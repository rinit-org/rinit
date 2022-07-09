use std::{
    self,
    collections::{
        HashMap,
        TryReserveError,
    },
    io,
    os::unix::prelude::{
        AsRawFd,
        RawFd,
    },
    process::Stdio,
    ptr,
};

use async_pidfd::PidFd;
use async_recursion::async_recursion;
use async_scoped_local::TokioScope;
use indexmap::IndexMap;
use rinit_ipc::request_error::{
    DependencyFailedToStartSnafu,
    DependencyGraphNotFoundSnafu,
    DependentsStillRunningSnafu,
    LogicError,
    RequestError,
    ServiceFailedToStartSnafu,
    ServiceNotFoundSnafu,
};
use rinit_service::{
    config::Config,
    graph::DependencyGraph,
    service_state::ServiceState,
    types::Service,
};
use snafu::{
    ensure,
    IntoError,
    OptionExt,
    ResultExt,
    Snafu,
};
use tokio::{
    io::unix::AsyncFd,
    process::Command,
};
use tokio_stream::StreamExt;

use crate::live_service::LiveService;

pub struct LiveServiceGraph {
    pub live_services: IndexMap<String, LiveService>,
    config: Config,
}

#[derive(Snafu, Debug)]
pub enum SystemError {
    #[snafu(display("error reading dependency graph from disk: {source}"))]
    ReadGraphError { source: io::Error },
    #[snafu(display("error deserializing json: {source}"))]
    JsonDeserializeError { source: serde_json::Error },
    #[snafu(display("error when joining tasks: {source}"))]
    JoinError { source: tokio::task::JoinError },
    #[snafu(display("error when creating a pidfd: {source}"))]
    PidFdError { source: io::Error },
    #[snafu(display("error when sending a signal through: {source}"))]
    PidFdSendSignalError { source: io::Error },
    #[snafu(display("error when waiting on a pidfd: {source}"))]
    PidFdWaitError { source: io::Error },
    #[snafu(display("error when spawning the supervisor: {source}"))]
    SpawnError { source: io::Error },
    #[snafu(display("error when allocating into the memory: {source}"))]
    TryReserveError { source: TryReserveError },
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

pub fn pidfd_send_signal(
    pidfd: RawFd,
    signal: i32,
) -> io::Result<()> {
    unsafe {
        let ret = libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd,
            signal,
            ptr::null_mut() as *mut libc::c_char,
            0,
        );
        if ret == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(ret)
        }
    }?;

    Ok(())
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
    pub fn new(config: Config) -> Result<Self> {
        let graph_file = config.get_graph_filename();
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
        })
    }

    pub async fn start_all_services(&self) -> Vec<Result<()>> {
        // This is unsafe because the futures may outlive the current scope
        // We wait on them afterwards and we know that self will outlive them
        // so it's safe to use it
        let (_, futures) = unsafe {
            TokioScope::scope_and_collect(|s| {
                self.live_services.iter().for_each(|(_, live_service)| {
                    s.spawn(async move {
                        if live_service.node.service.should_start() {
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
            self.start_dependencies(live_service).await?;
            self.start_service_impl(live_service).await?;
        }
        let state = live_service.get_final_state().await;
        ensure!(
            state == ServiceState::Up,
            ServiceFailedToStartSnafu {
                service: live_service.node.name().to_string(),
            },
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
                let dep_service = self.live_services.get(dep).unwrap();
                if matches!(
                    dep_service.get_final_state().await,
                    ServiceState::Reset | ServiceState::Down
                ) {
                    // Awaiting here is safe, as starting services always mean spawning ks-run-*
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

    async fn start_service_impl(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        self.wait_on_deps_starting(live_service).await?;
        let res = match &live_service.node.service {
            Service::Oneshot(_) => Some("--oneshot=start"),
            Service::Longrun(_) => Some("--longrun=start"),
            Service::Bundle(_) => None,
            Service::Virtual(_) => None,
        };
        if let Some(supervise) = res {
            // TODO: Add logging and remove unwrap
            let child = loop {
                let res = Command::new("rsupervision")
                    .args(vec![
                        supervise,
                        &format!(
                            "--logdir={}",
                            self.config.logdir.as_ref().unwrap().to_string_lossy()
                        ),
                        &serde_json::to_string(&live_service.node.service).unwrap(),
                    ])
                    .stdin(Stdio::null())
                    .spawn();
                match res {
                    Ok(child) => break child,
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                    Err(err) => return Err(SpawnSnafu.into_error(err).into()),
                }
            };

            live_service.pidfd.replace(Some(
                AsyncFd::new(PidFd::from_pid(child.id().unwrap() as i32).unwrap()).unwrap(),
            ));
        }

        Ok(())
    }

    async fn wait_on_deps_starting(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        for dep in live_service.node.service.dependencies() {
            let dep_service = &self.live_services[dep];
            let state = dep_service.get_final_state().await;
            ensure!(
                state == ServiceState::Up,
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
        self.stop_service_impl(live_service).await
    }

    async fn stop_service_impl(
        &self,
        live_service: &LiveService,
    ) -> Result<()> {
        match &live_service.node.service {
            Service::Oneshot(_) => {
                // TODO: Add logging and remove unwrap
                Command::new("rsupervision")
                    .args(vec![
                        "--oneshot=stop",
                        &format!(
                            "--logdir={}",
                            self.config.logdir.as_ref().unwrap().to_string_lossy()
                        ),
                        &serde_json::to_string(&live_service.node.service).unwrap(),
                    ])
                    .stdin(Stdio::null())
                    .spawn()
                    .unwrap();
            }
            Service::Longrun(_) => {
                if let Some(pidfd) = live_service.pidfd.take() {
                    // TODO: Add timeout
                    pidfd_send_signal(pidfd.as_raw_fd(), 9)
                        .with_context(|_| PidFdSendSignalSnafu)?;
                    let _ready = pidfd.readable().await.unwrap();
                    pidfd.get_ref().wait().with_context(|_| PidFdWaitSnafu)?;
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
                for (_, live_service) in &self.live_services {
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
            .map(|dependant| -> &LiveService {
                self.live_services.get_index(*dependant).unwrap().1
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
        let graph_file = self.config.get_graph_filename();
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
                    self.live_services[&name].remove = true;
                }
                // This service is in both graph, update it now/later
                (true, true) => {
                    let new_live_service =
                        LiveService::new(dep_graph.nodes.swap_remove(&name).unwrap());
                    let state = *self.live_services[&name].state.borrow();
                    match state {
                        // If a service is already down or in reset state, just update it with
                        // the new one
                        ServiceState::Reset | ServiceState::Down => {
                            new_live_service.update_state(state);
                            self.live_services[&name] = new_live_service;
                            // Keep the current state
                        }
                        // otherwise, mark it for update. It will be updated by
                        // update_service_state
                        ServiceState::Up | ServiceState::Starting | ServiceState::Stopping => {
                            self.live_services[&name].new = Some(Box::new(new_live_service));
                        }
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
        state: ServiceState,
    ) -> Result<()> {
        let live_service = self.get_service(name)?;
        live_service.update_state(state);
        live_service.tx.send(state).unwrap();
        Ok(())
    }

    pub fn update_service(
        &mut self,
        name: &str,
    ) -> Result<()> {
        let live_service = self.live_services.get(name).with_context(|| {
            ServiceNotFoundSnafu {
                service: name.to_string(),
            }
        })?;

        // the service is marked for removal
        if live_service.remove {
            // remove in O(1)
            self.live_services.swap_remove(name);
        // There is a new version of this service
        } else if live_service.new.is_some() {
            let live_service = self.live_services.swap_remove(name).unwrap();
            let new_live_service = live_service.new.unwrap();
            new_live_service.update_state(ServiceState::Down);
            self.live_services
                .insert(name.to_string(), *new_live_service);
        }
        Ok(())
    }
}
