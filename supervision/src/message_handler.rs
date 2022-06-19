use std::io;

use futures::{
    prelude::*,
    stream::StreamExt,
};
use rinit_ipc::{
    Message,
    Reply,
};
use rinit_service::service_state::ServiceState;
use tokio::net::UnixStream;
use tracing::trace;

use crate::{
    live_service::LiveService,
    live_service_graph::LiveServiceGraph,
};

pub struct MessageHandler {
    graph: LiveServiceGraph,
}

impl MessageHandler {
    pub fn new(graph: LiveServiceGraph) -> Self {
        Self { graph }
    }

    pub async fn handle_stream<'a>(
        &'a self,
        stream: UnixStream,
    ) {
        let buf = Self::read(&stream).await;
        let message: Message = serde_json::from_slice(&buf).unwrap();
        trace!("Received message from socket: {message:?}");
        let reply = self.handle(message).await;
        self.write_stream(stream, reply).await;
    }

    pub async fn handle<'a>(
        &self,
        message: Message,
    ) -> Reply {
        match message {
            Message::ServiceIsUp(up, name) => {
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
            Message::ServicesStatus(services) => {
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
            Message::StartServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter(|service| {
                        if !self.graph.indexes.contains_key(service.as_str()) {
                            err.push_str(&format!("{service} not found\n"));
                            true
                        } else {
                            false
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
                        err.push_str(&format!("{e:#?}\n"));
                    }
                }
                Reply::Result(if !err.is_empty() { Some(err) } else { None })
            }
            Message::StopServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter(|service| {
                        if self.graph.indexes.contains_key(service.as_str()) {
                            err.push_str(&format!("{service} not found\n"));
                            true
                        } else {
                            false
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
                        err.push_str(&format!("{e:#?}\n"));
                    }
                }
                Reply::Result(if !err.is_empty() { Some(err) } else { None })
            }
            Message::StartAllServices => {
                self.graph.start_all_services().await;
                Reply::Empty
            }
            Message::StopAllServices => {
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
        stream: UnixStream,
        reply: Reply,
    ) {
        match stream.try_write(&serde_json::to_vec(&reply).unwrap()) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {
                todo!()
            }
        }
        match stream.try_write("\n".as_bytes()) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {
                todo!()
            }
        }
    }
}
