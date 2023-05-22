use futures::{
    prelude::*,
    stream::StreamExt,
};
use remoc::rch;
use rinit_ipc::{
    request_error::RequestError,
    ConnectionError as ConnectionErrorGeneric,
    Reply,
    Request,
};
use rinit_service::service_state::{
    IdleServiceState,
    ServiceState,
};
use tokio::{
    net::UnixStream,
    sync::{
        watch,
        RwLock,
    },
    task,
};
use tracing::error;

use crate::{
    live_service::LiveService,
    live_service_graph::{
        LiveGraphError,
        LiveServiceGraph,
    },
};

type ConnectionError = ConnectionErrorGeneric<Result<Reply, RequestError>>;

pub struct RequestHandler {
    graph: RwLock<LiveServiceGraph>,
    stop_ipc: watch::Sender<bool>,
}

impl RequestHandler {
    pub fn new(
        graph: LiveServiceGraph,
        stop_ipc: watch::Sender<bool>,
    ) -> Self {
        Self {
            graph: RwLock::new(graph),
            stop_ipc,
        }
    }

    // Read IPC messages (i.e. rctl)
    pub async fn handle_ipc_stream(
        &self,
        stream: UnixStream,
    ) -> Result<(), ConnectionError> {
        let (socket_rx, socket_tx) = stream.into_split();
        let (conn, mut tx, mut rx): (
            _,
            rch::base::Sender<Result<Reply, RequestError>>,
            rch::base::Receiver<Request>,
        ) = remoc::Connect::io(remoc::Cfg::default(), socket_rx, socket_tx).await?;
        // This has to be spawned in a different task, otherwise everything blocks
        task::spawn_local(conn);
        loop {
            let request = match rx.recv().await {
                Ok(val) => {
                    match val {
                        Some(req) => req,
                        // No new requests
                        None => break,
                    }
                }
                Err(err) => {
                    match err {
                        // The connection terminated, break out of the loop
                        rch::base::RecvError::Receive(err) if err.is_terminated() => break,
                        rch::base::RecvError::Receive(_)
                        | rch::base::RecvError::Deserialize(_)
                        | rch::base::RecvError::MissingPorts(_) => {
                            return Err(ConnectionError::ReceiveError { source: err });
                        }
                    }
                }
            };
            let reply = self.handle_request(request).await;
            tx.send(reply).await?;
        }

        Ok(())
    }

    pub async fn handle_request<'a>(
        &self,
        request: Request,
    ) -> Result<Reply, RequestError> {
        let graph = self.graph.read().await;
        Ok(match request {
            Request::ServicesStatus => {
                let services: Vec<Result<&LiveService, LiveGraphError>> = graph
                    .live_services
                    .iter()
                    .map(|(_, live_service)| live_service)
                    .map(Result::Ok)
                    .collect();
                let states = stream::iter(services)
                    .then(async move |res| {
                        match res {
                            Ok(live_service) => {
                                Ok((
                                    live_service.node.name().to_owned(),
                                    *live_service.state.borrow(),
                                ))
                            }
                            Err(err) => Err(err),
                        }
                    })
                    .collect::<Vec<_>>()
                    .await;
                Reply::ServicesStates(states.into_iter().collect::<Result<Vec<_>, _>>()?)
            }
            Request::ServiceStatus(service) => {
                let state = graph.get_service(&service)?.wait_idle_state();
                drop(graph);
                Reply::ServiceState(service.clone(), ServiceState::Idle(state.await))
            }
            Request::StartService { service, runlevel } => {
                graph.check_runlevel(&service, runlevel)?;
                graph.start_service(graph.get_service(&service)?).await?;
                let state = graph.get_service(&service)?.wait_idle_state();
                drop(graph);
                Reply::Success(state.await == IdleServiceState::Up)
            }
            Request::StopService { service, runlevel } => {
                graph.check_runlevel(&service, runlevel)?;
                graph.stop_service(graph.get_service(&service)?).await?;
                let state = graph.get_service(&service)?.wait_idle_state();
                drop(graph);
                Reply::Success(state.await == IdleServiceState::Down)
            }
            Request::StartAllServices => {
                graph
                    .start_all_services(rinit_service::types::RunLevel::Boot)
                    .await;
                graph
                    .start_all_services(rinit_service::types::RunLevel::Default)
                    .await;
                Reply::Empty
            }
            // This request can be generated by rctl or by sending a SIGTERM/SIGINT
            Request::StopAllServices => {
                // Stop listening to IPC requests
                if let Err(err) = self.stop_ipc.send(true) {
                    error!("could not stop listening on IPC socket: {err}");
                }
                graph
                    .stop_all_services(rinit_service::types::RunLevel::Default)
                    .await;
                graph
                    .stop_all_services(rinit_service::types::RunLevel::Boot)
                    .await;
                Reply::Empty
            }
            Request::ReloadGraph => {
                drop(graph);
                let mut graph = self.graph.write().await;
                graph.reload_dependency_graph().await?;
                Reply::Empty
            }
            Request::UpdateServiceStatus(name, state) => {
                graph.update_service_state(&name, state)?;
                // To update the service, we need the get a write lock
                // Only get it if needed
                if state == IdleServiceState::Down {
                    drop(graph);
                    let mut graph = self.graph.write().await;
                    graph.update_service(&name)?;
                }
                Reply::Empty
            }
        })
    }
}
