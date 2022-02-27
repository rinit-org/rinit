use std::{
    collections::HashMap,
    process::Stdio,
    sync::Arc,
};

use anyhow::Result;
use async_recursion::async_recursion;
use kansei_core::{
    graph::DependencyGraph,
    types::Service,
};
use tokio::{
    fs::{
        self,
        File,
    },
    io::AsyncWriteExt,
    process::Command,
    sync::{
        RwLock,
        RwLockWriteGuard,
    },
};

use crate::{
    live_service::{
        LiveService,
        ServiceStatus,
    },
    CONFIG,
};

pub struct LiveServiceGraph {
    indexes: HashMap<String, usize>,
    live_services: RwLock<Vec<Arc<RwLock<LiveService>>>>,
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
            live_services: RwLock::new(
                nodes
                    .into_iter()
                    .map(|node| Arc::new(RwLock::new(node)))
                    .collect(),
            ),
        })
    }

    pub async fn start_all_services(&'static self) {
        let services = self.live_services.read().await;
        let futures: Vec<_> = services
            .clone()
            .into_iter()
            .map(|live_service| {
                let live_service = live_service.to_owned();
                tokio::spawn(async move {
                    if live_service.read().await.node.service.should_start() {
                        println!("name: {}", live_service.read().await.node.name());
                        self.start_service_impl(&mut live_service.write().await)
                            .await;
                    }
                })
            })
            .collect();
        for future in futures {
            future.await.unwrap();
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
                    .unwrap();
                let res = {
                    let lock = dep_service.read().await;
                    let status = lock.status.lock().await;
                    *status != ServiceStatus::Up
                        && *status != ServiceStatus::Starting
                        && *status != ServiceStatus::Stopping
                };
                if res {
                    self.start_service(&mut dep_service.write().await).await;
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
        let res = match &live_service.node.service {
            Service::Oneshot(oneshot) => Some(("ks-run-oneshot", bincode::serialize(&oneshot))),
            Service::Longrun(longrun) => Some(("ks-run-longrun", bincode::serialize(&longrun))),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some((exe, ser_res)) = res {
            let config = CONFIG.read().await;
            let runtime_service_dir = config
                .as_ref()
                .rundir
                .as_ref()
                .unwrap()
                .join(&live_service.node.name());
            fs::create_dir_all(&runtime_service_dir).await.unwrap();
            let service_path = runtime_service_dir.join("service");
            let mut file = File::create(service_path).await.unwrap();
            let buf = ser_res.unwrap();
            file.write(&buf).await.unwrap();
            // TODO: Add logging and remove unwrap
            Command::new(exe)
                .args(vec![runtime_service_dir])
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
