use serde::{
    Deserialize,
    Serialize,
};
use snafu::{
    futures::TryFutureExt,
    whatever,
    ResultExt,
    Snafu,
};
use tokio::{
    io,
    net::UnixStream,
};

use crate::get_host_address;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    ServiceIsUp(bool, String),
    ServicesStatus(Vec<String>),
    StartServices(Vec<String>),
    StopServices(Vec<String>),
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(whatever, display("{message}"))]
    Whatever {
        message: String,
        #[snafu(source(from(Box<dyn std::error::Error + Send + Sync>, Some)))]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl Message {
    pub async fn send(self) -> Result<Vec<u8>, Error> {
        self.send_to(get_host_address()).await
    }

    async fn send_to(
        self,
        socket: &str,
    ) -> Result<Vec<u8>, Error> {
        let stream = UnixStream::connect(socket)
            .with_whatever_context(|_| format!("unable to accept connection to {socket}"))
            .await?;

        let _ready = stream
            .writable()
            .with_whatever_context(|_| format!("unable to accept connection to {socket}"))
            .await?;

        let message = serde_json::to_vec(&self).whatever_context("error serializing message")?;
        match stream.try_write(&message) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                whatever!("error sending message: {}", e);
            }
        }
        match stream.try_write("\n".as_bytes()) {
            Ok(_) => {}
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                whatever!("error sending message: {}", e);
            }
        }

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

        Ok(res)
    }
}
