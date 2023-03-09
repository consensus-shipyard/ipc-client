// Copyright 2022-2023 Protocol Labs
// SPDX-License-Identifier: MIT
//! List subnets in gateway actor

use crate::manager::{SubnetInfo, SubnetManager};
use crate::server::handlers::manager::subnet::SubnetManagerPool;
use crate::server::JsonRPCRequestHandler;
use anyhow::anyhow;
use async_trait::async_trait;
use ipc_sdk::subnet_id::SubnetID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct ListSubnetsParams {
    pub subnet: String,
}

/// The create subnet json rpc method handler.
pub(crate) struct ListSubnetsHandler {
    pool: Arc<SubnetManagerPool>,
}

impl ListSubnetsHandler {
    pub(crate) fn new(pool: Arc<SubnetManagerPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl JsonRPCRequestHandler for ListSubnetsHandler {
    type Request = ListSubnetsParams;
    type Response = HashMap<SubnetID, SubnetInfo>;

    async fn handle(&self, request: Self::Request) -> anyhow::Result<Self::Response> {
        let conn = match self.pool.get(&request.subnet) {
            None => return Err(anyhow!("target parent subnet not found")),
            Some(conn) => conn,
        };

        let subnet = SubnetID::from_str(&request.subnet)?;
        conn.manager().list_child_subnets(subnet).await
    }
}
