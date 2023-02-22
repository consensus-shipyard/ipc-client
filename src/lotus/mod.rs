pub mod client;
///! The lotus api to interact with lotus node
pub mod message;
#[cfg(test)]
mod tests;

use anyhow::Result;
use async_trait::async_trait;
use cid::Cid;
use fvm_shared::address::Address;
use serde::de::DeserializeOwned;
use std::fmt::Debug;

pub use crate::lotus::client::LotusJsonRPCClient;
pub use crate::lotus::message::{MpoolPushMessage, MpoolPushMessageResponseInner};
pub use crate::lotus::message::{
    ReadStateResponse, StateWaitMsgResponse, WalletKeyType, WalletListResponse,
};

/// The network version of lotus network.
/// see https://github.com/filecoin-project/go-state-types/blob/f6fd668a32b4b4a0bc39fd69d8a5f8fb11f49461/network/version.go#L7
pub type NetworkVersion = u32;

/// The Lotus client api to interact with the Lotus node.
#[async_trait]
pub trait LotusClient {
    /// Push the message to memory pool, see: https://lotus.filecoin.io/reference/lotus/mpool/#mpoolpushmessage
    async fn mpool_push_message(
        &self,
        msg: MpoolPushMessage,
    ) -> Result<MpoolPushMessageResponseInner>;

    /// Wait for the message cid of a particular nonce, see: https://lotus.filecoin.io/reference/lotus/state/#statewaitmsg
    async fn state_wait_msg(&self, cid: Cid, nonce: u64) -> Result<StateWaitMsgResponse>;

    /// Returns the name of the network the node is synced to, see https://lotus.filecoin.io/reference/lotus/state/#statenetworkname
    async fn state_network_name(&self) -> Result<String>;

    /// Returns the network version at the given tipset, see https://lotus.filecoin.io/reference/lotus/state/#statenetworkversion
    async fn state_network_version(&self, tip_sets: Vec<Cid>) -> Result<NetworkVersion>;

    /// Get the default wallet of the node, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletdefaultaddress
    async fn wallet_default(&self) -> Result<Address>;

    /// List the wallets in the node, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletlist
    async fn wallet_list(&self) -> Result<WalletListResponse>;

    /// Create a new wallet, see: https://lotus.filecoin.io/reference/lotus/wallet/#walletnew
    async fn wallet_new(&self, key_type: WalletKeyType) -> Result<String>;

    /// Read the state of the address at tipset, see: https://lotus.filecoin.io/reference/lotus/state/#statereadstate
    async fn read_state<State: DeserializeOwned + Debug>(
        &self,
        address: Address,
        tipset: Cid,
    ) -> Result<ReadStateResponse<State>>;
}
