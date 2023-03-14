// Copyright 2022-2023 Protocol Labs
// SPDX-License-Identifier: MIT
//! Kill a subnet cli command handler.

use async_trait::async_trait;
use clap::Args;
use std::fmt::Debug;

use crate::cli::commands::get_ipc_agent_url;
use crate::cli::{CommandLineHandler, GlobalArguments};
use crate::config::json_rpc_methods;
use crate::jsonrpc::{JsonRpcClient, JsonRpcClientImpl};
use crate::server::KillSubnetParams;

/// The command to kill an existing subnet.
pub(crate) struct KillSubnet;

#[async_trait]
impl CommandLineHandler for KillSubnet {
    type Arguments = KillSubnetArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("kill subnet with args: {:?}", arguments);

        let url = get_ipc_agent_url(&arguments.ipc_agent_url, global)?;
        let json_rpc_client = JsonRpcClientImpl::new(url, None);

        let params = KillSubnetParams {
            subnet: arguments.subnet.clone(),
            from: arguments.from.clone(),
        };

        json_rpc_client
            .request::<()>(json_rpc_methods::KILL_SUBNET, serde_json::to_value(params)?)
            .await?;

        log::info!("killed subnet: {:}", arguments.subnet);

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Kill an existing subnet")]
pub(crate) struct KillSubnetArgs {
    #[arg(help = "The JSON RPC server url for ipc agent")]
    pub ipc_agent_url: Option<String>,
    #[arg(help = "The address that kills the subnet")]
    pub from: Option<String>,
    #[arg(help = "The subnet to kill")]
    pub subnet: String,
}
