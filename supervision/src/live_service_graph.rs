use std::{
    collections::HashMap,
    ffi::OsStr,
    os::unix::prelude::AsRawFd,
    process::Stdio,
    sync::Arc,
};

use anyhow::{
    ensure,
    Context,
    Result,
};
use async_pidfd::PidFd;
use async_recursion::async_recursion;
use rinit_service::{
    graph::DependencyGraph,
    service_state::ServiceState,
    types::Service,
};
use tokio::{
    fs::{
        self,
        File,
    },
    io::{
        unix::AsyncFd,
        AsyncWriteExt,
    },
    process::Command,
    sync::RwLock,
};
use tracing::{
    error,
    warn,
};

use crate::{
    live_service::LiveService,
    pidfd_send_signal::pidfd_send_signal,
    CONFIG,
};

pub struct LiveServiceGraph {
    pub indexes: HashMap<String, usize>,
    pub live_services: RwLock<Vec<Arc<LiveService>>>,
}

impl LiveServiceGraph {
    pub fn new(graph: DependencyGraph) -> Result<Self> {
        let nodes: Vec<_> = graph
            .nodes
            .into_iter()
            .map(LiveService::new)
            .map(Arc::new)
            .collect();
        Ok(Self {
            indexes: nodes
                .iter()
                .enumerate()
                .map(|(i, el)| (el.node.name().to_owned(), i))
                .collect(),
            live_services: RwLock::new(nodes),
        })
    }

    pub async fn start_all_services(&'static self) {
        let services = self.live_services.read().await;
        let futures: Vec<_> = services
            .iter()
            .map(|live_service| {
                let live_service = live_service.clone();
                tokio::spawn(async move {
                    if live_service.node.service.should_start() {
                        // TODO: Generate an order of the services to start and use
                        // start_service_impl
                        let res = self.start_service(live_service.clone()).await;
                        if let Err(err) = res {
                            warn!("{err:?}");
                        }
                    }
                })
            })
            .collect();
        for future in futures {
            if let Err(err) = future.await {
                if err.is_panic() {
                    error!("{err:?}");
                }
            }
        }
    }
    #[async_recursion]
    pub async fn start_service(
        &self,
        live_service: Arc<LiveService>,
    ) -> Result<()> {
        let mut state = live_service.state.clone().lock_owned().await;
        if *state == ServiceState::Up {
            return Ok(());
        }
        while *state == ServiceState::Stopping {
            state = live_service
                .wait
                .wait((state, live_service.state.clone()))
                .await;
        }
        if *state != ServiceState::Starting {
            *state = ServiceState::Starting;
            drop(state);
            self.start_dependencies(&live_service).await?;
            self.start_service_impl(live_service.clone()).await?;
        }
        let state = live_service.get_final_state().await;
        ensure!(
            state == ServiceState::Up,
            "service {} failed to start",
            live_service.node.name(),
        );
        Ok(())
    }

    async fn start_dependencies(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let futures: Vec<_> = live_service
            .node
            .service
            .dependencies()
            .iter()
            .map(async move |dep| -> Result<()> {
                let services = self.live_services.read().await;
                let dep_service = services.get(*self.indexes.get(dep).unwrap()).unwrap();
                if match dep_service.get_final_state().await {
                    ServiceState::Reset | ServiceState::Down => true,
                    _ => false,
                } {
                    // Awaiting here is safe, as starting services always mean spawning ks-run-*
                    self.start_service(dep_service.clone()).await
                } else {
                    Ok(())
                }
            })
            .collect();
        for future in futures {
            future.await?;
        }

        Ok(())
    }

    async fn start_service_impl(
        &self,
        live_service: Arc<LiveService>,
    ) -> Result<()> {
        self.wait_on_deps_starting(live_service.clone())
            .await
            .with_context(|| format!("while starting service {}", live_service.node.name()))?;
        let res = match &live_service.node.service {
            Service::Oneshot(oneshot) => Some(("oneshot", serde_json::to_vec(&oneshot))),
            Service::Longrun(longrun) => Some(("longrun", serde_json::to_vec(&longrun))),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some((supervise, ser_res)) = res {
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
            let child = Command::new("rsvc")
                .args(vec![
                    supervise,
                    "start",
                    runtime_service_dir.to_str().unwrap(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .spawn()
                .unwrap();

            let mut status = live_service.status.lock().await;
            status.pidfd = Some(AsyncFd::new(
                PidFd::from_pid(child.id().unwrap() as i32)
                    .context("unable to create PidFd from child pid")?,
            )?);
        }

        Ok(())
    }

    async fn wait_on_deps_starting(
        &self,
        live_service: Arc<LiveService>,
    ) -> Result<()> {
        let futures: Vec<_> = live_service
            .node
            .service
            .dependencies()
            .iter()
            .map(async move |dep| -> (&str, ServiceState) {
                let dep_service = self.get_service(dep).await;
                (dep, dep_service.get_final_state().await)
            })
            .collect();

        for future in futures {
            let (dep, state) = future.await;
            ensure!(
                state == ServiceState::Up,
                "dependency {dep} could not start successfully",
            );
        }

        Ok(())
    }

    #[async_recursion]
    pub async fn stop_service(
        &self,
        live_service: Arc<LiveService>,
    ) -> Result<()> {
        self.stop_service_impl(live_service).await?;
        Ok(())
    }

    async fn stop_service_impl(
        &self,
        live_service: Arc<LiveService>,
    ) -> Result<()> {
        self.wait_on_deps_stopping(&live_service);
        match &live_service.node.service {
            Service::Oneshot(oneshot) => {
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
                let buf = serde_json::to_vec(&oneshot).unwrap();
                file.write(&buf).await.unwrap();
                // TODO: Add logging and remove unwrap
                Command::new("ks-run-oneshot")
                    .args(vec![runtime_service_dir.as_os_str(), OsStr::new("stop")])
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .spawn()
                    .unwrap();
            }
            Service::Longrun(_) => {
                if let Some(pidfd) = &live_service.status.lock().await.pidfd {
                    // TODO: Add timeout
                    pidfd_send_signal(pidfd.as_raw_fd(), 9)
                        .with_context(|| format!("unable to send signal {:?}", 15))?;
                    let _ready = pidfd.readable().await.unwrap();
                    pidfd.get_ref().wait().context("unable to call waitid")?;
                }
            }
            Service::Bundle(_) => {}
            Service::Virtual(_) => {}
        }

        Ok(())
    }

    pub async fn stop_all_services(&'static self) {
        let services = self.live_services.read().await;
        let futures: Vec<_> = services
            .iter()
            .map(|live_service| {
                let live_service = live_service.clone();
                tokio::spawn(async move {
                    if live_service.get_final_state().await == ServiceState::Up {
                        // TODO: Log
                        self.stop_service(live_service.clone()).await;
                    }
                })
            })
            .collect();
        for future in futures {
            future.await.unwrap();
        }
    }

    pub async fn get_service(
        &self,
        name: &str,
    ) -> Arc<LiveService> {
        self.live_services
            .read()
            .await
            .get(*self.indexes.get(name).expect("This should never happen"))
            .unwrap()
            .clone()
    }

    async fn wait_on_deps_stopping(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let services = self.live_services.read().await;
        let futures: Vec<_> = live_service
            .node
            .dependents
            .iter()
            .map(|dependant| -> Arc<LiveService> { services.get(*dependant).unwrap().to_owned() })
            .map(async move |dependant| -> ServiceState { dependant.get_final_state().await })
            .collect();

        for future in futures {
            let state = future.await;
            ensure!(
                state == ServiceState::Down || state == ServiceState::Reset,
                "error",
            );
        }

        Ok(())
    }
}
