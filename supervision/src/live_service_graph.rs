use std::{
    self,
    collections::HashMap,
    io,
    os::unix::prelude::AsRawFd,
    process::Stdio,
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
use snafu::{
    ResultExt,
    Snafu,
};
use tokio::{
    io::unix::AsyncFd,
    process::Command,
};
use tokio_stream::StreamExt;

use crate::{
    live_service::LiveService,
    pidfd_send_signal::pidfd_send_signal,
};

pub struct LiveServiceGraph {
    pub indexes: HashMap<String, usize>,
    pub live_services: Vec<LiveService>,
}

#[derive(Debug, Snafu)]
pub enum SystemError {
    #[snafu()]
    ReadGraphError {
        source: io::Error,
    },
    JsonDeserializeError {
        source: serde_json::Error,
    },
    SystemJoinError {
        source: tokio::task::JoinError,
    },
    PidFdError {
        source: io::Error,
    },
    PidFdSendSignalError {
        source: io::Error,
    },
    PidFdWaitError {
        source: io::Error,
    },
}

#[derive(Debug, Snafu)]
pub enum LogicError {
    #[snafu()]
    DependencyFailedToStart {
        service: String,
        dependency: String,
    },
    #[snafu()]
    DependentsStillRunning {
        service: String,
        dependents: Vec<String>,
    },
    ServiceFailedToStart {
        service: String,
    },
}

type Result<T> = std::result::Result<std::result::Result<T, LogicError>, SystemError>;

macro_rules! ok {
    ($val:expr) => {
        Ok(Ok($val))
    };
}

macro_rules! try_ {
    ($expr:expr) => {
        let res = $expr?;
        if res.is_err() {
            return Ok(res);
        }
    };
}

macro_rules! ensure_logic {
    ($test:expr, $err: expr) => {
        if $test {
            return Ok($err.fail());
        }
    };
}

impl LiveServiceGraph {
    pub fn new(config: Config) -> Result<Self> {
        let graph_file = config.get_graph_filename();
        let graph: DependencyGraph = if graph_file.exists() {
            serde_json::from_slice(&std::fs::read(graph_file).with_context(|_| ReadGraphSnafu {})?)
                .with_context(|_| JsonDeserializeSnafu {})?
        } else {
            DependencyGraph::new()
        };
        let nodes: Vec<_> = graph.nodes.into_iter().map(LiveService::new).collect();
        Ok(Ok(Self {
            indexes: nodes
                .iter()
                .enumerate()
                .map(|(i, el)| (el.node.name().to_owned(), i))
                .collect(),
            live_services: nodes,
        }))
    }

    pub async fn start_all_services(&self) -> Vec<Result<()>> {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                self.live_services.iter().for_each(|live_service| {
                    s.spawn(async move {
                        if live_service.node.service.should_start() {
                            // TODO: Generate an order of the services to start and use
                            // start_service_impl
                            self.start_service(live_service).await
                        } else {
                            ok!(())
                        }
                    });
                });
            })
        }
        .await;
        futures
            .into_iter()
            // Here we either lose the system error or the join error
            // let's consider the join error (which could even be a panic)
            // more important
            .map(|res| res.with_context(|_| SystemJoinSnafu {})?)
            .collect()
    }

    #[async_recursion(?Send)]
    pub async fn start_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let mut state = *live_service.state.borrow();
        if state == ServiceState::Up {
            return ok!(());
        }
        while state == ServiceState::Stopping {
            state = live_service.get_final_state().await;
        }
        // Check that the service is not already starting
        // or is already up. Some other task could have done so while awaiting above
        if state != ServiceState::Starting && state != ServiceState::Up {
            live_service.state.replace(ServiceState::Starting);
            try_!(self.start_dependencies(live_service).await);
            try_!(self.start_service_impl(live_service).await);
        }
        let state = live_service.get_final_state().await;
        ensure_logic!(
            state == ServiceState::Up,
            ServiceFailedToStartSnafu {
                service: live_service.node.name()
            }
        );
        ok!(())
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
                    self.start_service(dep_service).await
                } else {
                    ok!(())
                }
            })
            .collect();
        for future in futures {
            try_!(future.await);
        }

        ok!(())
    }

    async fn start_service_impl(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        try_!(self.wait_on_deps_starting(live_service).await);
        let res = match &live_service.node.service {
            Service::Oneshot(oneshot) => Some(("oneshot", serde_json::to_string(&oneshot))),
            Service::Longrun(longrun) => Some(("longrun", serde_json::to_string(&longrun))),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some((supervise, ser_res)) = res && let Ok(json) = &ser_res {
            // TODO: Add logging and remove unwrap
            let child = loop {
            let res = Command::new("rsvc")
                .args(vec![supervise, "start", json])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .spawn()
                ;
                match res {
                    Ok(child) => break child,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {},
                    Err(_) => unreachable!()
                }
            };

            live_service.pidfd.replace(Some(AsyncFd::new(
                PidFd::from_pid(child.id().unwrap() as i32)
            .unwrap()).unwrap()));
        }

        ok!(())
    }

    async fn wait_on_deps_starting(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        for dep in live_service.node.service.dependencies() {
            let dep_service = self.get_service(dep);
            let state = dep_service.get_final_state().await;
            ensure_logic!(
                state != ServiceState::Up,
                DependencyFailedToStartSnafu {
                    service: live_service.node.name(),
                    dependency: dep
                }
            );
        }

        ok!(())
    }

    pub async fn stop_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let dependents = self.get_dependents(live_service);
        try_!(Self::wait_on_dependents_stopping(live_service.node.name(), &dependents).await);
        self.stop_service_impl(live_service).await
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
                if let Some(pidfd) = live_service.pidfd.take() {
                    // TODO: Add timeout
                    pidfd_send_signal(pidfd.as_raw_fd(), 9)
                        .with_context(|_| PidFdSendSignalSnafu {})?;
                    let _ready = pidfd.readable().await.unwrap();
                    pidfd.get_ref().wait().with_context(|_| PidFdWaitSnafu {})?;
                }
            }
            Service::Bundle(_) => {}
            Service::Virtual(_) => {}
        }

        ok!(())
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
                            self.stop_service(live_service).await.unwrap().unwrap();
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

    async fn wait_on_dependents_stopping(
        name: &str,
        dependents: &[&LiveService],
    ) -> Result<()> {
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
            .map(|live_service| live_service.node.name().to_owned())
            .collect::<Vec<String>>()
            .await;

        ensure_logic!(
            dependents_running.is_empty(),
            DependentsStillRunningSnafu {
                service: name,
                dependents: dependents_running
            }
        );

        ok!(())
    }
}
