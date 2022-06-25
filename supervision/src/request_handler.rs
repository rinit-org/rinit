use std::io;

use futures::{
    prelude::*,
    stream::StreamExt,
};
use rinit_ipc::{
    request_error::RequestError,
    Reply,
    Request,
};
use rinit_service::service_state::ServiceState;
use tokio::{
    io::AsyncWriteExt,
    net::UnixStream,
};
use tracing::trace;

use crate::{
    live_service::LiveService,
    live_service_graph::{
        LiveGraphError,
        LiveServiceGraph,
    },
};

pub struct RequestHandler {
    graph: LiveServiceGraph,
}

impl RequestHandler {
    pub fn new(graph: LiveServiceGraph) -> Self {
        Self { graph }
    }

    pub async fn handle_stream(
        &self,
        mut stream: UnixStream,
    ) {
        loop {
            let buf = Self::read(&stream).await;
            let request: Request = serde_json::from_slice(&buf).unwrap();
            trace!("Received request from socket: {request:?}");
            let reply = self.handle(request).await;
            self.write_stream(&mut stream, reply).await;
        }
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
                        ServiceState::Up
                    } else {
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

    async fn read(stream: &UnixStream) -> Vec<u8> {
        let mut res = Vec::new();
        loop {
            stream.readable().await.unwrap();

            let mut buf = [0; 1024];
            match stream.try_read(&mut buf) {
                Ok(size) if size == 0 => break,
                Ok(_) => {
                    let index = buf.iter().position(|&c| c == 10);
                    res.extend_from_slice(&buf[..index.unwrap_or(buf.len())]);
                    if index.is_some() {
                        break;
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    todo!()
                }
            }
        }

        res
    }

    pub async fn write_stream(
        &self,
        stream: &mut UnixStream,
        reply: Result<Reply, RequestError>,
    ) {
        stream
            .write_all(&serde_json::to_vec(&reply).unwrap())
            .await
            .unwrap();
        stream.write_all("\n".as_bytes()).await.unwrap();
    }
}
