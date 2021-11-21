use std::collections::HashMap;

use anyhow::Result;
use async_recursion::async_recursion;
use async_std::{
    process::{
        Command,
        Stdio,
    },
    sync::{
        RwLock,
        RwLockUpgradableReadGuard,
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
        let services = self.live_services.read().await;
        let futures: Vec<_> = services
            .iter()
            .map(async move |live_service| {
                if live_service.read().await.node.service.should_start() {
                    self.start_service_impl(&mut live_service.write().await)
                        .await;
                }
            })
            .collect();
        for future in futures {
            future.await;
        }
    }

    #[async_recursion]
    async fn start_service(
        &self,
        live_service: &mut RwLockWriteGuard<'_, LiveService>,
    ) {
        live_service.change_status(ServiceStatus::Starting).await;
        self.start_dependencies(live_service).await;
        self.start_service_impl(live_service).await;
    }

    async fn start_dependencies(
        &self,
        live_service: &mut RwLockWriteGuard<'_, LiveService>,
    ) {
        // Start dependencies
        let futures: Vec<_> = live_service
            .node
            .service
            .dependencies()
            .iter()
            .map(async move |dep| {
                let services = self.live_services.read().await;
                let dep_service = services
                    .get(*self.indexes.get(dep).expect("This should nevel happen"))
                    .unwrap()
                    .upgradable_read()
                    .await;
                let res = {
                    let status = dep_service.status.lock().await;
                    *status != ServiceStatus::Up
                        && *status != ServiceStatus::Starting
                        && *status != ServiceStatus::Stopping
                };
                if res {
                    self.start_service(&mut RwLockUpgradableReadGuard::upgrade(dep_service).await)
                        .await;
                }
            })
            .collect();
        for future in futures {
            future.await;
        }
    }

    async fn start_service_impl(
        &self,
        live_service: &mut RwLockWriteGuard<'_, LiveService>,
    ) {
        self.wait_on_deps(&*live_service).await;
        let exe = match &live_service.node.service {
            Service::Oneshot(_) => Some("ks-run-oneshot"),
            Service::Longrun(_) => Some("ks-run-longrun"),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some(exe) = exe {
            // TODO: Add logging and remove unwrap
            Command::new(exe)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .spawn()
                .unwrap();
        }
    }

    async fn wait_on_deps(
        &self,
        live_service: &LiveService,
    ) {
        let futures: Vec<_> = live_service
            .node
            .service
            .dependencies()
            .iter()
            .map(async move |dep| {
                let services = self.live_services.read().await;
                let dep_service = services
                    .get(*self.indexes.get(dep).expect("This should nevel happen"))
                    .unwrap()
                    .read()
                    .await;
                dep_service.wait_on_status().await
            })
            .collect();

        for future in futures {
            future.await;
        }
    }
}
