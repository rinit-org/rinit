use std::{
    io,
    sync::Arc,
};

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
    pub graph: &'static LiveServiceGraph,
}

impl MessageHandler {
    pub async fn handle(
        &self,
        stream: UnixStream,
    ) {
        let buf = Self::read(&stream).await;
        let message: Message = serde_json::from_slice(&buf).unwrap();
        trace!("Received message {message:?}");
        match message {
            Message::ServiceIsUp(up, name) => {
                let live_service = self.graph.get_service(&name).await;
                let mut state = live_service.state.lock().await;
                *state = if up {
                    ServiceState::Up
                } else {
                    ServiceState::Down
                };
                live_service.wait.notify_all();
            }
            Message::ServicesStatus(services) => {
                let services: Vec<Arc<LiveService>> = if services.is_empty() {
                    stream::iter(self.graph.live_services.read().await.iter())
                        .then(async move |live_service| live_service.clone())
                        .collect()
                        .await
                } else {
                    stream::iter(&services)
                        .then(async move |service| self.graph.get_service(&service).await)
                        .collect()
                        .await
                };
                let states = stream::iter(services)
                    .then(async move |live_service| {
                        let state = live_service.state.lock().await;
                        (live_service.node.name().to_owned(), state.to_owned())
                    })
                    .collect::<Vec<_>>()
                    .await;
                let reply = Reply::ServicesStates(states);
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
            Message::StartServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter_map(|service| {
                        if !self.graph.indexes.contains_key(service) {
                            err.push_str(&format!("{service} not found\n"));
                            Some(service)
                        } else {
                            None
                        }
                    })
                    .map(async move |service| {
                        self.graph
                            .start_service(self.graph.get_service(service).await)
                    })
                    .collect::<Vec<_>>();
                for f in futures {
                    if let Err(e) = f.await.await {
                        err.push_str(&format!("{e:#?}\n"));
                    }
                }
                let reply = Reply::Result(if !err.is_empty() { Some(err) } else { None });
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
            Message::StopServices(services) => {
                let mut err = String::new();
                let futures = services
                    .iter()
                    .filter_map(|service| {
                        if self.graph.indexes.contains_key(service) {
                            err.push_str(&format!("{service} not found\n"));
                            Some(service)
                        } else {
                            None
                        }
                    })
                    .map(async move |service| {
                        self.graph
                            .stop_service(self.graph.get_service(service).await)
                    })
                    .collect::<Vec<_>>();
                for f in futures {
                    if let Err(e) = f.await.await {
                        err.push_str(&format!("{e:#?}\n"));
                    }
                }
                let reply = Reply::Result(if !err.is_empty() { Some(err) } else { None });
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

    pub fn new(graph: &'static LiveServiceGraph) -> Self {
        Self { graph }
    }
}
