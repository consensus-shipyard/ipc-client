// Copyright 2022-2023 Protocol Labs
// SPDX-License-Identifier: MIT
//! Leave subnet cli command handler.

use async_trait::async_trait;
use clap::Args;
use std::fmt::Debug;

use crate::cli::commands::get_ipc_agent_url;
use crate::cli::{CommandLineHandler, GlobalArguments};
use crate::config::json_rpc_methods;
use crate::jsonrpc::{JsonRpcClient, JsonRpcClientImpl};
use crate::server::LeaveSubnetParams;

/// The command to leave a new subnet.
pub(crate) struct LeaveSubnet;

#[async_trait]
impl CommandLineHandler for LeaveSubnet {
    type Arguments = LeaveSubnetArgs;

    async fn handle(global: &GlobalArguments, arguments: &Self::Arguments) -> anyhow::Result<()> {
        log::debug!("leave subnet with args: {:?}", arguments);

        let url = get_ipc_agent_url(&arguments.ipc_agent_url, global)?;
        let json_rpc_client = JsonRpcClientImpl::new(url, None);

        let params = LeaveSubnetParams {
            subnet: arguments.subnet.clone(),
            from: arguments.from.clone(),
        };

        json_rpc_client
            .request::<()>(
                json_rpc_methods::LEAVE_SUBNET,
                serde_json::to_value(params)?,
            )
            .await?;

        log::info!("left subnet: {:}", arguments.subnet);

        Ok(())
    }
}

#[derive(Debug, Args)]
#[command(about = "Leaving a subnet")]
pub(crate) struct LeaveSubnetArgs {
    #[arg(help = "The JSON RPC server url for ipc agent")]
    pub ipc_agent_url: Option<String>,
    #[arg(help = "The address that leaves the subnet")]
    pub from: Option<String>,
    #[arg(help = "The subnet to leave")]
    pub subnet: String,
}
