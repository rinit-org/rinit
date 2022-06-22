use std::io;

use futures::{
    prelude::*,
    stream::StreamExt,
};
use rinit_ipc::{
    Request,
    Reply,
};
use rinit_service::service_state::ServiceState;
use tokio::{
    io::AsyncWriteExt,
    net::UnixStream,
};
use tracing::trace;

use crate::{
    live_service::LiveService,
    live_service_graph::LiveServiceGraph,
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
        stream: UnixStream,
    ) {
        let buf = Self::read(&stream).await;
        let request: Request = serde_json::from_slice(&buf).unwrap();
        trace!("Received request from socket: {request:?}");
        let reply = self.handle(request).await;
        self.write_stream(stream, reply).await;
    }

    pub async fn handle<'a>(
        &self,
        request: Request,
    ) -> Reply {
        match request {
            Request::ServiceIsUp(up, name) => {
                let live_service = self.graph.get_service(&name);
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
            Request::ServicesStatus(services) => {
                let services: Vec<&LiveService> = if services.is_empty() {
                    self.graph.live_services.iter().collect()
                } else {
                    services
                        .iter()
                        .map(|service| self.graph.get_service(service))
                        .collect()
                };
                let states = stream::iter(services)
                    .then(async move |live_service| {
                        (
                            live_service.node.name().to_owned(),
                            *live_service.state.borrow(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .await;
                Reply::ServicesStates(states)
            }
            Request::StartServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter(|service| {
                        if !self.graph.indexes.contains_key(service.as_str()) {
                            err = format!("{err}\n{service} not found");
                            false
                        } else {
                            true
                        }
                    })
                    .map(async move |service| {
                        self.graph
                            .start_service(self.graph.get_service(service))
                            .await
                    })
                    .collect::<Vec<_>>();
                for f in futures {
                    if let Err(e) = f.await {
                        err = format!("{err}\n{e}");
                    }
                }
                Reply::Result(if !err.is_empty() { Some(err) } else { None })
            }
            Request::StopServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter(|service| {
                        if !self.graph.indexes.contains_key(service.as_str()) {
                            err = format!("{err}\n{service} not found");
                            false
                        } else {
                            true
                        }
                    })
                    .map(async move |service| {
                        self.graph
                            .stop_service(self.graph.get_service(service))
                            .await
                    })
                    .collect::<Vec<_>>();
                for f in futures {
                    if let Err(e) = f.await {
                        err = format!("{err}\n{e}");
                    }
                }
                Reply::Result(if !err.is_empty() { Some(err) } else { None })
            }
            Request::StartAllServices => {
                self.graph.start_all_services().await;
                Reply::Empty
            }
            Request::StopAllServices => {
                self.graph.stop_all_services().await;
                Reply::Empty
            }
        }
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
        mut stream: UnixStream,
        reply: Reply,
    ) {
        if matches!(reply, Reply::Empty) {
            return;
        }
        stream
            .write_all(&serde_json::to_vec(&reply).unwrap())
            .await
            .unwrap();
        stream.write_all("\n".as_bytes()).await.unwrap();
    }
}
