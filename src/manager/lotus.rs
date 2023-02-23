use std::collections::HashMap;
use std::str::FromStr;

use crate::jsonrpc::JsonRpcClient;
use crate::lotus::{LotusClient, LotusJsonRPCClient, MpoolPushMessage, StateWaitMsgResponse};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use cid::Cid;
use fil_actors_runtime::{builtin::singletons::INIT_ACTOR_ADDR, cbor};
use fvm_shared::{address::Address, econ::TokenAmount, MethodNum};
use ipc_gateway::Checkpoint;
use ipc_sdk::subnet_id::SubnetID;
use ipc_subnet_actor::{ConstructParams, JoinParams, types::MANIFEST_ID};
use crate::manager::params::{ExecParams, INIT_EXEC_METHOD_NUM};
use super::subnet::{SubnetInfo, SubnetManager};

pub struct LotusSubnetManager<T: JsonRpcClient> {
    lotus_client: LotusJsonRPCClient<T>,
}

#[async_trait]
impl<T: JsonRpcClient + Send + Sync> SubnetManager for LotusSubnetManager<T> {
    async fn create_subnet(&self, from: Address, params: ConstructParams) -> Result<Address> {
        if self.is_network_match(&params.parent).await? {
            return Err(anyhow!("subnet actor being deployed in the wrong parent network, parent network names do not match"));
        }

        let actor_code_cid = self.get_subnet_actor_code_cid().await?;
        let constructor_params = cbor::serialize(&params, "create subnet actor")?;

        let exec_params = ExecParams {
            code_cid: actor_code_cid,
            constructor_params,
        };
        log::debug!("create subnet for init actor with params: {exec_params:?}");
        let init_params = cbor::serialize(&exec_params, "init subnet actor params")?;
        let message = MpoolPushMessage::new(
            INIT_ACTOR_ADDR,
            from,
            INIT_EXEC_METHOD_NUM,
            init_params.to_vec(),
        );

        let response = self.mpool_push_and_wait(message).await?;
        let address_raw = response.receipt.result;
        log::info!("created subnet at address: {address_raw:}");

        Ok(Address::from_str(&address_raw)?)
    }

    async fn join_subnet(
        &self,
        subnet: SubnetID,
        from: Address,
        collateral: TokenAmount,
        params: JoinParams,
    ) -> Result<()> {
        if self.is_network_match(&subnet).await? {
            return Err(anyhow!("subnet actor being deployed in the wrong parent network, parent network names do not match"));
        }

        let to = subnet.subnet_actor();
        let mut message = MpoolPushMessage::new(
            to,
            from,
            ipc_subnet_actor::Method::Join as MethodNum,
            cbor::serialize(&params, "join subnet params")?.to_vec(),
        );
        message.value = collateral;

        self.mpool_push_and_wait(message).await?;
        log::info!("joined subnet: {subnet:}");

        Ok(())
    }

    async fn leave_subnet(&self, subnet: SubnetID, from: Address) -> Result<()> {
        if self.is_network_match(&subnet).await? {
            return Err(anyhow!("subnet actor being deployed in the wrong parent network, parent network names do not match"));
        }

        self.mpool_push_and_wait(MpoolPushMessage::new(
            subnet.subnet_actor(),
            from,
            ipc_subnet_actor::Method::Leave as MethodNum,
            vec![],
        )).await?;
        log::info!("left subnet: {subnet:}");

        Ok(())
    }

    async fn kill_subnet(&self, subnet: SubnetID, from: Address) -> Result<()> {
        if self.is_network_match(&subnet).await? {
            return Err(anyhow!("subnet actor being deployed in the wrong parent network, parent network names do not match"));
        }

        self.mpool_push_and_wait(MpoolPushMessage::new(
            subnet.subnet_actor(),
            from,
            ipc_subnet_actor::Method::Kill as MethodNum,
            vec![],
        )).await?;
        log::info!("left subnet: {subnet:}");

        Ok(())
    }

    async fn submit_checkpoint(
        &self,
        _subnet: SubnetID,
        _from: Address,
        _ch: Checkpoint,
    ) -> Result<()> {
        panic!("not implemented")
    }

    async fn list_child_subnets(&self, _subnet: SubnetID) -> Result<HashMap<SubnetID, SubnetInfo>> {
        panic!("not implemented")
    }
}

impl<T: JsonRpcClient + Send + Sync> LotusSubnetManager<T> {
    pub fn new(lotus_client: LotusJsonRPCClient<T>) -> Self {
        Self { lotus_client }
    }

    /// Publish the message to memory pool and wait for the response
    async fn mpool_push_and_wait(&self, message: MpoolPushMessage) -> Result<StateWaitMsgResponse> {
        let mem_push_response = self.lotus_client.mpool_push_message(message).await?;

        let message_cid = mem_push_response.cid()?;
        let nonce = mem_push_response.nonce;
        log::debug!("message published with cid: {message_cid:?} and nonce: {nonce:?}");

        self.lotus_client.state_wait_msg(message_cid, nonce).await
    }

    /// Checks the `network` is the one we are currently talking to.
    async fn is_network_match(&self, network: &SubnetID) -> Result<bool> {
        let network_name = self.lotus_client.state_network_name().await?;
        Ok(network.to_string() == network_name)
    }

    /// Obtain the actor code cid of `ipc_subnet_actor` only, since this is the
    /// code cid we are interested in.
    async fn get_subnet_actor_code_cid(&self) -> Result<Cid> {
        let network_version = self.lotus_client.state_network_version(vec![]).await?;

        let mut cid_map = self
            .lotus_client
            .state_actor_code_cids(network_version)
            .await?;

        cid_map
            .remove(MANIFEST_ID)
            .ok_or_else(|| anyhow!("actor cid not found"))
    }
}
