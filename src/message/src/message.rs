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

        let mut buf = Vec::new();
        buf.reserve(2048);
        loop {
            let _ready = stream
                .readable()
                .with_whatever_context(|_| format!("unable to accept connection to {socket}"))
                .await?;

            match stream.try_read(buf.as_mut_slice()) {
                Ok(size) if size == 0 => break,
                Ok(size) => buf.reserve(buf.len() + size),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    whatever!("error sending message: {}", e);
                }
            }
        }

        Ok(buf)
    }
}
