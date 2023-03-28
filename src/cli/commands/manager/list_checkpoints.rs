// Copyright 2022-2023 Protocol Labs
// SPDX-License-Identifier: MIT
//! List checkpoints cli command

use async_trait::async_trait;
use clap::Args;
use fvm_shared::clock::ChainEpoch;
use ipc_gateway::Checkpoint;
use std::fmt::Debug;

use crate::cli::commands::get_ipc_agent_url;
use crate::cli::{CommandLineHandler, GlobalArguments};
use crate::config::json_rpc_methods;
use crate::jsonrpc::{JsonRpcClient, JsonRpcClientImpl};
use crate::server::list_checkpoints::ListCheckpointsParams;

/// The command to list checkpoints committed in a subnet actor.
pub(crate) struct ListCheckpoints;

#[async_trait]
impl CommandLineHandler for ListCheckpoints {
    type Arguments = ListCheckpointsArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("list checkpoints with args: {:?}", arguments);

        let url = get_ipc_agent_url(&arguments.ipc_agent_url, global)?;
        let json_rpc_client = JsonRpcClientImpl::new(url, None);

        let params = ListCheckpointsParams {
            subnet_id: arguments.subnet_id.clone(),
            from_epoch: arguments.from_epoch,
            to_epoch: arguments.to_epoch,
        };

        let checkpoints = json_rpc_client
            .request::<Vec<Checkpoint>>(
                json_rpc_methods::LIST_CHECKPOINTS,
                serde_json::to_value(params)?,
            )
            .await?;

        log::info!("list checkpoints: {checkpoints:?}");

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "List checkpoints")]
pub(crate) struct ListCheckpointsArgs {
    #[arg(long, short, help = "The JSON RPC server url for ipc agent")]
    pub ipc_agent_url: Option<String>,
    #[arg(long, short, help = "The subnet id of the checkpointing subnet")]
    pub subnet_id: String,
    #[arg(long, short, help = "Include checkpoints from this epoch")]
    pub from_epoch: ChainEpoch,
    #[arg(long, short, help = "Include checkpoints up to this epoch")]
    pub to_epoch: ChainEpoch,
}
