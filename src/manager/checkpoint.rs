use std::collections::{HashMap, HashSet};
// Copyright 2022-2023 Protocol Labs
// SPDX-License-Identifier: MIT
use std::collections::hash_map::RandomState;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use cid::Cid;
use fil_actors_runtime::cbor;
use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use fvm_shared::address::Address;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::MethodNum;
use ipc_gateway::checkpoint::checkpoint_epoch;
use ipc_gateway::{BottomUpCheckpoint, Checkpoint};
use ipc_sdk::subnet_id::SubnetID;
use primitives::TCid;
use reqwest::redirect::Policy;
use tokio::select;
use tokio::sync::Notify;
use tokio::time::sleep;
use tokio_graceful_shutdown::{IntoSubsystem, SubsystemHandle};

use crate::config::{ReloadableConfig, Subnet};
use crate::jsonrpc::JsonRpcClient;
use crate::lotus::client::LotusJsonRPCClient;
use crate::lotus::message::mpool::MpoolPushMessage;
use crate::lotus::LotusClient;
use crate::manager::policy::{CheckpointPolicy, OptimisticCheckpointPolicy};
use crate::manager::subnet::BottomUpCheckpointManager;
use crate::manager::LotusSubnetManager;

/// The frequency at which to check a new chain head.
const CHAIN_HEAD_REQUEST_PERIOD: Duration = Duration::from_secs(10);

/// The `CheckpointSubsystem`. When run, it actively monitors subnets and submits checkpoints.
pub struct CheckpointSubsystem {
    /// The subsystem uses a `ReloadableConfig` to ensure that, at all, times, the subnets under
    /// management are those in the latest version of the config.
    config: Arc<ReloadableConfig>,
}

impl CheckpointSubsystem {
    /// Creates a new `CheckpointSubsystem` with a configuration `config`.
    pub fn new(config: Arc<ReloadableConfig>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl IntoSubsystem<anyhow::Error> for CheckpointSubsystem {
    /// Runs the checkpoint subsystem, which actively monitors subnets and submits checkpoints.
    /// For each (account, subnet) that exists in the config, the subnet is monitored and checkpoints
    /// are submitted at the appropriate epochs.
    async fn run(self, subsys: SubsystemHandle) -> Result<()> {
        // Each event in this channel is notification of a new config.
        let mut config_chan = self.config.new_subscriber();

        loop {
            // Load the latest config.
            let config = self.config.get_config();

            // Create a `manage_subnet` future for each (child, parent) subnet pair under management
            // and collect them in a `FuturesUnordered` set.
            let mut manage_subnet_futures = FuturesUnordered::new();
            let stop_subnet_managers = Arc::new(Notify::new());
            for (child, parent) in subnets_to_manage(&config.subnets) {
                manage_subnet_futures
                    .push(manage_subnet((child, parent), stop_subnet_managers.clone()));
            }

            log::debug!("We have {} subnets to manage", manage_subnet_futures.len());

            // Spawn a task to drive the `manage_subnet` futures.
            let manage_subnets_task = tokio::spawn(async move {
                loop {
                    match manage_subnet_futures.next().await {
                        Some(Ok(())) => {}
                        Some(Err(e)) => {
                            log::error!("Error in manage_subnet: {}", e);
                        }
                        None => {
                            log::debug!("All manage_subnet_futures have finished");
                            break;
                        }
                    }
                }
            });

            // Watch for shutdown requests and config changes.
            let is_shutdown = select! {
                _ = subsys.on_shutdown_requested() => {
                    log::info!("Shutting down checkpointing subsystem");
                    true
                },
                r = config_chan.recv() => {
                    log::info!("Config changed, reloading checkpointing subsystem");
                    match r {
                        Ok(_) => { false },
                        Err(_) => {
                            log::error!("Config channel unexpectedly closed, shutting down checkpointing subsystem");
                            true
                        },
                    }
                },
            };

            // Cleanly stop the `manage_subnet` futures.
            stop_subnet_managers.notify_waiters();
            log::debug!("Waiting for subnet managers to finish");
            manage_subnets_task.await?;

            if is_shutdown {
                return anyhow::Ok(());
            }
        }
    }
}

/// This function takes a `HashMap<String, Subnet>` and returns a `Vec` of tuples of the form
/// `(child_subnet, parent_subnet)`, where `child_subnet` is a subnet that we need to actively
/// manage checkpoint for. This means that for each `child_subnet` there exists at least one account
/// for which we need to submit checkpoints on behalf of to `parent_subnet`, which must also be
/// present in the map.
fn subnets_to_manage(subnets_by_id: &HashMap<SubnetID, Subnet>) -> Vec<(Subnet, Subnet)> {
    // We filter for subnets that have at least one account and for which the parent subnet
    // is also in the map, and map into a Vec of (child_subnet, parent_subnet) tuples.
    subnets_by_id
        .values()
        .filter(|s| !s.accounts.is_empty())
        .filter(|s| s.id.parent().is_some() && subnets_by_id.contains_key(&s.id.parent().unwrap()))
        .map(|s| (s.clone(), subnets_by_id[&s.id.parent().unwrap()].clone()))
        .collect()
}

/// Monitors a subnet `child` for checkpoint blocks. It emits an event for every new checkpoint block.
async fn manage_subnet((child, parent): (Subnet, Subnet), stop_notify: Arc<Notify>) -> Result<()> {
    log::info!(
        "Starting checkpoint manager for (child, parent) subnet pair ({}, {})",
        child.id,
        parent.id
    );

    let child_client = LotusJsonRPCClient::from_subnet(&child);
    let parent_client = LotusJsonRPCClient::from_subnet(&parent);

    let checkpoint_period = get_checkpoint_period(&parent, &parent_client).await?;
    let validators = get_validators(&child.accounts, &parent_client).await?;
    if validators.is_empty() {
        log::error!("no validator in subnet");
        return Ok(());
    }

    let child_manager = LotusSubnetManager::new(child_client);
    let parent_manager = LotusSubnetManager::new(parent_client);

    let policy = OptimisticCheckpointPolicy::new(
        child.id.clone(),
        &parent_manager,
        &child_manager,
        checkpoint_period,
    );

    'outer: loop {
        // validators not be empty at this stage
        'inner: for validator in validators.iter() {
            // now process the submission
            select! {
                next_epoch = policy.next_submission_epoch(validator) => {
                    let checkpoint = child_manager.create_bu_checkpoint_template(&child, next_epoch).await?;
                    policy.submit_checkpoint(validator.clone(), checkpoint).await?;
                }
                _ = stop_notify.notified() => { break 'outer; }

            }
        }
    }

    Ok(())
}

/// Get the checkpoint period in the parent subnet
async fn get_checkpoint_period<T: JsonRpcClient + Send + Sync>(
    parent_subnet: &Subnet,
    client: &LotusJsonRPCClient<T>,
) -> Result<ChainEpoch> {
    log::debug!("Getting parent tipset");

    let chain_head = client.chain_head().await?;
    let cid_map = chain_head.cids.first().unwrap().clone();
    let parent_tip_set = Cid::try_from(cid_map)?;

    let state = client
        .ipc_read_subnet_actor_state(&child.id, parent_tip_set)
        .await
        .map_err(|e| {
            log::error!("error getting subnet actor state for {:?}", &child.id);
            e
        })?;
    Ok(state.check_period)
}

async fn get_validators<T: JsonRpcClient + Send + Sync>(
    child_subnet_accounts: &[Address],
    parent_client: &LotusJsonRPCClient<T>,
) -> Result<Vec<Address>> {
    let parent_head = parent_client.chain_head().await?;

    // A key assumption we make now is that each block has exactly one tip set. We panic
    // if this is not the case as it violates our assumption.
    // TODO: update this logic once the assumption changes (i.e., mainnet)
    assert_eq!(parent_head.cids.len(), 1);
    let cid_map = parent_head.cids.first().unwrap().clone();
    let parent_tip_set = Cid::try_from(cid_map)?;

    let subnet_actor_state = parent_client
        .ipc_read_subnet_actor_state(&child.id, parent_tip_set)
        .await?;

    match subnet_actor_state.validator_set.validators {
        None => Ok(vec![]),
        Some(validators) => Ok(validators
            .iter()
            .filter(|v| child_subnet_accounts.contains(&Address::from_str(&v.addr)?))
            .collect()),
    }
}
