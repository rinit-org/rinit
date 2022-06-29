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
    task,
};

use crate::{
    live_service::LiveService,
    live_service_graph::{
        LiveGraphError,
        LiveServiceGraph,
    },
};

type ConnectionError = ConnectionErrorGeneric<Result<Reply, RequestError>>;

pub struct RequestHandler {
    graph: LiveServiceGraph,
}

impl RequestHandler {
    pub fn new(graph: LiveServiceGraph) -> Self {
        Self { graph }
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
        Ok(match request {
            Request::ServiceIsUp(name, up) => {
                let live_service = self.graph.get_service(&name)?;
                live_service.update_state(
                    if up {
                        tracing::info!("{name} is up!");
                        ServiceState::Up
                    } else {
                        tracing::info!("{name} is down!");
                        ServiceState::Down
                    },
                );
                live_service.tx.send(()).unwrap();
                Reply::Empty
            }
            Request::ServicesStatus() => {
                let services: Vec<Result<&LiveService, LiveGraphError>> =
                    self.graph.live_services.iter().map(Result::Ok).collect();
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
                Reply::ServiceState(
                    service.clone(),
                    self.graph.get_service(&service)?.get_final_state().await,
                )
            }
            Request::StartService(service) => {
                self.graph
                    .start_service(self.graph.get_service(&service)?)
                    .await?;
                Reply::Success(
                    self.graph.get_service(&service)?.get_final_state().await == ServiceState::Up,
                )
            }
            Request::StopService(service) => {
                self.graph
                    .stop_service(self.graph.get_service(&service)?)
                    .await?;
                Reply::Success(
                    self.graph.get_service(&service)?.get_final_state().await == ServiceState::Down,
                )
            }
            Request::StartAllServices => {
                self.graph.start_all_services().await;
                Reply::Empty
            }
            Request::StopAllServices => {
                self.graph.stop_all_services().await;
                Reply::Empty
            }
        })
    }
}
