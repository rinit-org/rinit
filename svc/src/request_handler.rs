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
use rinit_service::service_state::ServiceState;
use tokio::{
    net::UnixStream,
    sync::RwLock,
    task,
};
use tracing::info;

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
}

impl RequestHandler {
    pub fn new(graph: LiveServiceGraph) -> Self {
        Self {
            graph: RwLock::new(graph),
        }
    }

    pub async fn handle_stream(
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
            let reply = self.handle(request).await;
            tx.send(reply).await?;
        }

        Ok(())
    }

    pub async fn handle<'a>(
        &self,
        request: Request,
    ) -> Result<Reply, RequestError> {
        let graph = self.graph.read().await;
        Ok(match request {
            Request::ServiceIsUp(name, up) => {
                let new_state = if up {
                    info!("Service {name} is up");
                    ServiceState::Up
                } else {
                    info!("Service {name} is down");
                    ServiceState::Down
                };
                graph.update_service_state(&name, new_state)?;
                if matches!(new_state, ServiceState::Down) {
                    drop(graph);
                    let mut graph = self.graph.write().await;
                    graph.update_service(&name)?;
                }
                Reply::Empty
            }
            Request::ServicesStatus() => {
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
                let state = graph.get_service(&service)?.get_final_state();
                drop(graph);
                Reply::ServiceState(service.clone(), state.await)
            }
            Request::StartService { service, runlevel } => {
                graph.check_runlevel(&service, runlevel)?;
                graph.start_service(graph.get_service(&service)?).await?;
                let state = graph.get_service(&service)?.get_final_state();
                drop(graph);
                Reply::Success(state.await == ServiceState::Up)
            }
            Request::StopService { service, runlevel } => {
                graph.check_runlevel(&service, runlevel)?;
                graph.stop_service(graph.get_service(&service)?).await?;
                let state = graph.get_service(&service)?.get_final_state();
                drop(graph);
                Reply::Success(state.await == ServiceState::Down)
            }
            Request::StartAllServices(runlevel) => {
                graph.start_all_services(runlevel).await;
                Reply::Empty
            }
            Request::StopAllServices(runlevel) => {
                graph.stop_all_services(runlevel).await;
                Reply::Empty
            }
            Request::ReloadGraph => {
                drop(graph);
                let mut graph = self.graph.write().await;
                graph.reload_dependency_graph().await?;
                Reply::Empty
            }
        })
    }
}
