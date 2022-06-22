use std::{
    self,
    collections::HashMap,
    os::unix::prelude::AsRawFd,
    process::Stdio,
};

use anyhow::{
    ensure,
    Context,
    Result,
};
use async_pidfd::PidFd;
use async_recursion::async_recursion;
use async_scoped_local::TokioScope;
use rinit_service::{
    config::Config,
    graph::DependencyGraph,
    service_state::ServiceState,
    types::Service,
};
use tokio::{
    io::unix::AsyncFd,
    process::Command,
};
use tokio_stream::StreamExt;
use tracing::{
    error,
    warn,
};

use crate::{
    live_service::LiveService,
    pidfd_send_signal::pidfd_send_signal,
};

pub struct LiveServiceGraph {
    pub indexes: HashMap<String, usize>,
    pub live_services: Vec<LiveService>,
}

impl LiveServiceGraph {
    pub fn new(config: Config) -> Result<Self> {
        let graph: DependencyGraph =
            serde_json::from_slice(&std::fs::read(config.get_graph_filename()).unwrap()).unwrap();
        let nodes: Vec<_> = graph.nodes.into_iter().map(LiveService::new).collect();
        Ok(Self {
            indexes: nodes
                .iter()
                .enumerate()
                .map(|(i, el)| (el.node.name().to_owned(), i))
                .collect(),
            live_services: nodes,
        })
    }

    pub async fn start_all_services(&self) {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_res, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                for live_service in &self.live_services {
                    s.spawn(async move {
                        if live_service.node.service.should_start() {
                            // TODO: Generate an order of the services to start and use
                            // start_service_impl
                            let res = self.start_service(live_service).await;
                            if let Err(err) = res {
                                warn!("{err:?}");
                            }
                        }
                    });
                }
            })
        }
        .await;
        for future in futures {
            if let Err(err) = future {
                if err.is_panic() {
                    error!("{err:?}");
                }
            }
        }
    }

    #[async_recursion(?Send)]
    pub async fn start_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let mut state = *live_service.state.borrow();
        if state == ServiceState::Up {
            return Ok(());
        }
        while state == ServiceState::Stopping {
            state = live_service.get_final_state().await;
        }
        // Check that the service is not already starting
        // or is already up. Some other task could have done so while awaiting above
        if state != ServiceState::Starting && state != ServiceState::Up {
            live_service.state.replace(ServiceState::Starting);
            self.start_dependencies(&live_service).await?;
            self.start_service_impl(&live_service).await?;
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
                let dep_service = self
                    .live_services
                    .get(*self.indexes.get(dep).unwrap())
                    .unwrap();
                if matches!(
                    dep_service.get_final_state().await,
                    ServiceState::Reset | ServiceState::Down
                ) {
                    // Awaiting here is safe, as starting services always mean spawning ks-run-*
                    self.start_service(&dep_service).await
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
        live_service: &LiveService,
    ) -> Result<()> {
        self.wait_on_deps_starting(live_service)
            .await
            .with_context(|| format!("while starting service {}", live_service.node.name()))?;
        let res = match &live_service.node.service {
            Service::Oneshot(oneshot) => Some(("oneshot", serde_json::to_string(&oneshot))),
            Service::Longrun(longrun) => Some(("longrun", serde_json::to_string(&longrun))),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some((supervise, ser_res)) = res && let Ok(json) = &ser_res {
            // TODO: Add logging and remove unwrap
            let child = Command::new("rsvc")
                .args(vec![supervise, "start", &json])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .spawn()
                .unwrap();

            live_service.pidfd.replace(Some(AsyncFd::new(
                PidFd::from_pid(child.id().unwrap() as i32)
                    .context("unable to create PidFd from child pid")?,
            )?));
        }

        Ok(())
    }

    async fn wait_on_deps_starting(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let futures: Vec<_> = live_service
            .node
            .service
            .dependencies()
            .iter()
            .map(async move |dep| -> (&str, ServiceState) {
                let dep_service = self.get_service(dep);
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

    pub async fn stop_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let dependents = self.get_dependents(live_service);
        Self::wait_on_dependents_stopping(&dependents).await?;
        self.stop_service_impl(&live_service).await
    }

    async fn stop_service_impl(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        match &live_service.node.service {
            Service::Oneshot(oneshot) => {
                // TODO: Add logging and remove unwrap
                Command::new("rsvc")
                    .args(vec![
                        "oneshot",
                        "stop",
                        &serde_json::to_string(&oneshot).unwrap(),
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .spawn()
                    .unwrap();
            }
            Service::Longrun(_) => {
                if let Some(pidfd) = &*live_service.pidfd.borrow() {
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

    pub async fn stop_all_services(&self) {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_res, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                for live_service in &self.live_services {
                    s.spawn(async move {
                        if live_service.get_final_state().await == ServiceState::Up {
                            // TODO: Log
                            self.stop_service(live_service).await.unwrap();
                        }
                    });
                }
            })
        }
        .await;
        for future in futures {
            future.unwrap();
        }
    }

    pub fn get_service(
        &self,
        name: &str,
    ) -> &LiveService {
        self.live_services
            .get(*self.indexes.get(name).expect("This should never happen"))
            .unwrap()
            .clone()
    }

    pub fn get_mut_service(
        &mut self,
        name: &str,
    ) -> &mut LiveService {
        self.live_services
            .get_mut(*self.indexes.get(name).expect("This should never happen"))
            .unwrap()
    }

    fn get_dependents(
        &self,
        live_service: &LiveService,
    ) -> Vec<&LiveService> {
        live_service
            .node
            .dependents
            .iter()
            .map(|dependant| -> &LiveService {
                self.live_services.get(*dependant).unwrap().to_owned()
            })
            .collect()
    }

    async fn wait_on_dependents_stopping(dependents: &[&LiveService]) -> Result<()> {
        let dependents_running = tokio_stream::iter(dependents
            .iter())
            // Run this sequentially since we can't stop until each has been stopped
            .then(async move |dependent| -> (&LiveService, ServiceState) {
                (dependent, dependent.get_final_state().await)
            })
            .filter_map(|(dependent, state)|
                match state {
                ServiceState::Reset | ServiceState::Down => None,
                ServiceState::Up | ServiceState::Starting
                | ServiceState::Stopping=> Some(dependent),
            })
            .collect::<Vec<&LiveService>>()
            .await;

        ensure!(
            dependents_running.is_empty(),
            "Dependants are still running"
        );

        Ok(())
    }
}
