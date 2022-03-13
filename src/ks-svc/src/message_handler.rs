use std::io;

use kansei_message::Message;
use tokio::net::UnixStream;

use crate::{
    live_service::ServiceState,
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
