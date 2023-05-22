use std::{
    self,
    collections::{
        HashMap,
        TryReserveError,
    },
    io,
};

use async_recursion::async_recursion;
use async_scoped_local::TokioScope;
use indexmap::IndexMap;
use rinit_ipc::{
    request_error::{
        DependencyFailedToStartSnafu,
        DependencyGraphNotFoundSnafu,
        DependentsStillRunningSnafu,
        LogicError,
        RequestError,
        RunLevelMustMatchSnafu,
        ServiceFailedToStartSnafu,
        ServiceNotFoundSnafu,
    },
    Request,
};
use rinit_service::{
    config::Config,
    graph::DependencyGraph,
    service_state::{
        IdleServiceState,
        ServiceState,
        TransitioningServiceState,
    },
    types::RunLevel,
};
use snafu::{
    ensure,
    ResultExt,
    Snafu,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{
    info,
    trace,
    warn,
};

use crate::live_service::LiveService;

pub struct LiveServiceGraph {
    pub live_services: IndexMap<String, LiveService>,
    config: Config,
    send: mpsc::Sender<Request>,
}

#[derive(Snafu, Debug)]
pub enum SystemError {
    #[snafu(display("error reading dependency graph from disk: {source}"))]
    ReadGraphError { source: io::Error },
    #[snafu(display("error deserializing json: {source}"))]
    JsonDeserializeError { source: serde_json::Error },
    #[snafu(display("error when joining tasks: {source}"))]
    JoinError { source: tokio::task::JoinError },
    #[snafu(display("rsupervision is not in PATH"))]
    RSupervisionNotInPath,
    #[snafu(display("error when sending a signal: {source}"))]
    SendSignalError { source: nix::Error },
    #[snafu(display("error when spawning the supervisor: {source}"))]
    SpawnError { source: io::Error },
    #[snafu(display("error when allocating into the memory: {source}"))]
    TryReserveError { source: TryReserveError },
    #[snafu(display("error when waiting on a child: {source}"))]
    WaitError { source: io::Error },
}

// Snafu doesn't work with enums of enums
// https://github.com/shepmaster/snafu/issues/199
// Use structs as workaround
#[derive(Snafu, Debug)]
pub enum LiveGraphError {
    #[snafu(display("{err}"))]
    SystemError { err: SystemError },
    #[snafu(display("{err}"))]
    LogicError { err: LogicError },
}

impl From<LogicError> for LiveGraphError {
    fn from(e: LogicError) -> Self {
        LiveGraphError::LogicError { err: e }
    }
}

impl From<SystemError> for LiveGraphError {
    fn from(e: SystemError) -> Self {
        LiveGraphError::SystemError { err: e }
    }
}

impl From<LiveGraphError> for RequestError {
    fn from(e: LiveGraphError) -> Self {
        match e {
            LiveGraphError::SystemError { err } => {
                RequestError::SystemError {
                    err: format!("{err}"),
                }
            }
            LiveGraphError::LogicError { err } => RequestError::LogicError { err },
        }
    }
}

type Result<T> = std::result::Result<T, LiveGraphError>;

impl LiveServiceGraph {
    pub fn new(
        config: Config,
        send: mpsc::Sender<Request>,
    ) -> Result<Self> {
        let graph_file = config.dirs.graph_filename();
        let graph: DependencyGraph = if graph_file.exists() {
            serde_json::from_slice(&std::fs::read(graph_file).with_context(|_| ReadGraphSnafu)?)
                .with_context(|_| JsonDeserializeSnafu)?
        } else {
            DependencyGraph::new()
        };
        Ok(Self {
            live_services: graph
                .nodes
                .into_iter()
                .map(|(name, node)| (name, LiveService::new(node)))
                .collect(),
            config,
            send,
        })
    }

    pub async fn start_all_services(
        &self,
        runlevel: RunLevel,
    ) -> Vec<Result<()>> {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                self.live_services.iter().for_each(|(_, live_service)| {
                    s.spawn(async move {
                        if live_service.node.service.should_start()
                            && live_service.node.service.runlevel() == runlevel
                        {
                            // TODO: Generate an order of the services to start and use
                            // start_service_impl
                            self.start_service(live_service).await
                        } else {
                            Ok(())
                        }
                    });
                });
            })
        }
        .await;
        futures
            .into_iter()
            .map(|res| {
                // Here we either lose the system error or the join error
                // let's consider the join error (which could even be a panic)
                // more important
                res.with_context(|_| JoinSnafu)?
            })
            .collect()
    }

    #[async_recursion(?Send)]
    pub async fn start_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let mut state = *live_service.state.borrow();
        if state == ServiceState::Idle(IdleServiceState::Up) {
            return Ok(());
        }
        if matches!(state, ServiceState::Transitioning(_)) {
            state = ServiceState::Idle(live_service.wait_idle_state().await);
        }
        // If the service is down
        if state == ServiceState::Idle(IdleServiceState::Down) {
            trace!("starting service {}", live_service.node.name());
            live_service.state.replace(ServiceState::Transitioning(
                TransitioningServiceState::Starting,
            ));
            self.start_dependencies(live_service).await?;
            self.wait_on_deps_starting(live_service).await?;

            // Call the closure and let the new subscriber collect all the tracings
            let success = live_service
                .start_service(&self.config.dirs.logdir, self.send.clone())
                .await;
            if let Err(err) = self
                .send
                .send(Request::UpdateServiceStatus(
                    live_service.node.name().to_string(),
                    if success {
                        IdleServiceState::Up
                    } else {
                        IdleServiceState::Down
                    },
                ))
                .await
            {
                warn!("Could not update service status: {err}");
            }
        }
        let state = live_service.wait_idle_state().await;
        ensure!(
            state == IdleServiceState::Up,
            ServiceFailedToStartSnafu {
                service: live_service.node.name().to_string(),
            },
        );
        trace!("service {} is {}", live_service.node.name(), state);
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
                let dep_service = self.live_services.get(dep).unwrap();
                if dep_service.wait_idle_state().await == IdleServiceState::Down {
                    // Awaiting here is safe, as starting services always mean spawning rsupervisor
                    self.start_service(dep_service).await
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

    async fn wait_on_deps_starting(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        for dep in live_service.node.service.dependencies() {
            let dep_service = &self.live_services[dep];
            let state = dep_service.wait_idle_state().await;
            ensure!(
                state == IdleServiceState::Up,
                DependencyFailedToStartSnafu {
                    service: live_service.node.name().to_string(),
                    dependency: dep.to_string(),
                }
            )
        }

        Ok(())
    }

    pub async fn stop_service(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        let dependents = self.get_dependents(live_service);
        Self::wait_on_dependents_stopping(live_service.node.name(), &dependents).await?;
        live_service.state.replace(ServiceState::Transitioning(
            TransitioningServiceState::Stopping,
        ));
        live_service.stop_service(&self.config.dirs.logdir).await;
        if let Err(err) = self
            .send
            .send(Request::UpdateServiceStatus(
                live_service.node.name().to_string(),
                IdleServiceState::Down,
            ))
            .await
        {
            warn!("Could not update service status: {err}");
        }
        Ok(())
    }

    pub async fn stop_all_services(
        &self,
        runlevel: RunLevel,
    ) {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_res, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                for (service, live_service) in &self.live_services {
                    s.spawn(async move {
                        if live_service.node.service.runlevel() == runlevel {
                            let dependents = self.get_dependents(live_service);
                            for dependent in dependents {
                                // Wait until the dependent is down
                                // TODO: Log
                                while let Ok(IdleServiceState::Up) =
                                    dependent.tx.subscribe().recv().await
                                {
                                }
                            }
                            self.stop_service(live_service).await.unwrap();

                            // Self::stop_service only spawn the supervisor, we don't know if the
                            // service has stopped yet. Get the state of each one
                            if *live_service.state.borrow()
                                == ServiceState::Idle(IdleServiceState::Up)
                            {
                                if let Ok(IdleServiceState::Up) =
                                    live_service.tx.subscribe().recv().await
                                {
                                    warn!("service {service} didn't exit successfully");
                                }
                            }
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
    ) -> Result<&LiveService> {
        if self.live_services.contains_key(name) {
            Ok(&self.live_services[name])
        } else {
            ServiceNotFoundSnafu {
                service: name.to_string(),
            }
            .fail()?
        }
    }

    fn get_dependents(
        &self,
        live_service: &LiveService,
    ) -> Vec<&LiveService> {
        live_service
            .node
            .dependents
            .iter()
            .map(|dependant| &self.live_services[dependant])
            .collect()
    }

    async fn wait_on_dependents_stopping(
        name: &str,
        dependents: &[&LiveService],
    ) -> Result<()> {
        let dependents_running = tokio_stream::iter(dependents
            .iter())
            // Run this sequentially since we can't stop until each has been stopped
            .then(async move |dependent| -> (&LiveService, IdleServiceState) {
                (dependent, dependent.wait_idle_state().await)
            })
            .filter_map(|(dependent, state)|
                match state {
                    IdleServiceState::Down => None,
                    IdleServiceState::Up => Some(dependent),
                })
            .map(|live_service| live_service.node.name().to_owned())
            .collect::<Vec<String>>()
            .await;

        ensure!(
            dependents_running.is_empty(),
            DependentsStillRunningSnafu {
                service: name.to_string(),
                dependents: dependents_running,
            }
        );
        Ok(())
    }

    pub async fn reload_dependency_graph(&mut self) -> Result<()> {
        let graph_file = self.config.dirs.graph_filename();
        ensure!(
            graph_file.exists(),
            DependencyGraphNotFoundSnafu {
                path: graph_file.to_string_lossy()
            }
        );
        let mut dep_graph: DependencyGraph =
            serde_json::from_slice(&std::fs::read(graph_file).with_context(|_| ReadGraphSnafu)?)
                .with_context(|_| JsonDeserializeSnafu)?;

        // Assume that the depedency graph only contains services that are needed
        // and that is correct. This way we can skip checking dependencies and other
        // requirements, and skip directly to checking service per service.
        // https://github.com/danyspin97/tt/blob/master/src/svc/live_service_graph.cpp

        // We could have inserted the index instead of the boolean, but using
        // swap_remove means that we invalidate these indexes. swap_remove cost
        // O(1) and remove O(n) making it worth all the hashmap access
        let mut services: HashMap<String, (bool, bool)> = HashMap::new();
        for service in &self.live_services {
            services.insert(service.0.clone(), (true, false));
        }

        for service in &dep_graph.nodes {
            let left = services
                .get(service.0.as_str())
                .map(|(left, _)| *left)
                .unwrap_or(false);
            services.insert(service.0.clone(), (left, true));
        }

        // Reserve memory for new services in advance, do not panic on memory allocation
        // fail
        self.live_services
            .reserve(services.iter().filter(|(_, (_, right))| *right).count());
        let mut index = self.live_services.len();
        for service in services {
            let name = service.0;
            match service.1 {
                // There is a new service, add it to the graph without starting it
                (false, true) => {
                    let new = LiveService::new(dep_graph.nodes.swap_remove(&name).unwrap());
                    self.live_services.insert(name, new);
                    index += index;
                }
                // This service is only the live state and not in the new dependency graph
                // mark it for removal
                (true, false) => {
                    let state = *self.live_services[&name].state.borrow();
                    // If a service is already down, just update it with the new one
                    if matches!(state, ServiceState::Idle(IdleServiceState::Down)) {
                        self.live_services.swap_remove(&name);
                    } else {
                        self.live_services[&name].remove = true;
                    }
                }
                // This service is in both graph, update it now/later
                (true, true) => {
                    let new_live_service =
                        LiveService::new(dep_graph.nodes.swap_remove(&name).unwrap());
                    let state = *self.live_services[&name].state.borrow();
                    // If a service is already down, just update it with
                    // the new one
                    if state == ServiceState::Idle(IdleServiceState::Down) {
                        new_live_service.update_state(state);
                        self.live_services[&name] = new_live_service;
                        // Keep the current state
                    } else {
                        // otherwise, mark it for update. It will be updated by
                        // update_service_state
                        self.live_services[&name].new = Some(Box::new(new_live_service));
                    }
                }
                (false, false) => unreachable!(),
            }
        }

        Ok(())
    }

    pub fn update_service_state(
        &self,
        name: &str,
        state: IdleServiceState,
    ) -> Result<()> {
        info!("Service {name} is {state}");
        let live_service = self.get_service(name)?;
        live_service.update_state(ServiceState::Idle(state));
        live_service.tx.send(state).unwrap();
        Ok(())
    }

    pub fn update_service(
        &mut self,
        name: &str,
    ) -> Result<()> {
        let live_service = self.get_service(name)?;
        // the service is marked for removal
        if live_service.remove {
            // remove in O(1)
            self.live_services.swap_remove(name);
        // There is a new version of this service
        } else if live_service.new.is_some() {
            let entry = self.live_services.entry(name.to_string());
            // Update entry in-place
            entry.and_modify(|live_service| {
                let new_live_service = live_service.new.take().unwrap();
                new_live_service.update_state(ServiceState::Idle(IdleServiceState::Down));
                *live_service = *new_live_service;
            });
        }
        Ok(())
    }

    pub fn check_runlevel(
        &self,
        name: &str,
        runlevel: RunLevel,
    ) -> Result<()> {
        ensure!(
            self.get_service(name)?.node.service.runlevel() == runlevel,
            RunLevelMustMatchSnafu { service: name }
        );

        Ok(())
    }
}
