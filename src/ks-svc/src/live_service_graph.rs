use std::collections::HashMap;

use anyhow::Result;
use async_std::{
    prelude::*,
    stream,
    stream::StreamExt,
    sync::{
        RwLock,
        RwLockWriteGuard,
    },
};
use kansei_core::{
    graph::DependencyGraph,
    types::Service,
};

use crate::live_service::{
    LiveService,
    ServiceStatus,
};

pub struct LiveServiceGraph {
    indexes: HashMap<String, usize>,
    live_services: RwLock<Vec<RwLock<LiveService>>>,
}

impl LiveServiceGraph {
    pub fn new(graph: DependencyGraph) -> Result<Self> {
        let nodes = graph
            .nodes
            .into_iter()
            .map(LiveService::new)
            .collect::<Vec<_>>();
        Ok(Self {
            indexes: nodes
                .iter()
                .enumerate()
                .map(|(i, el)| (el.node.name().to_owned(), i))
                .collect(),
            live_services: RwLock::new(nodes.into_iter().map(RwLock::new).collect()),
        })
    }

    pub async fn start_all_services(&self) {
        stream::from_iter(self.live_services.read().await.iter()).map(async move |live_service| {
            if live_service.read().await.node.service.should_start() {
                self.start_service(&mut live_service.write().await).await;
            }
        });
    }

    async fn start_service(
        &self,
        live_service: &mut RwLockWriteGuard<'_, LiveService>,
    ) {
        live_service.change_status(ServiceStatus::Up);
        let exe = match &live_service.node.service {
            Service::Oneshot(_) => Some("ks-run-oneshot"),
            Service::Longrun(_) => Some("ks-run-longrun"),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some(exe) = exe {
            Command::new(exe)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .spawn();
        }
    }
}
