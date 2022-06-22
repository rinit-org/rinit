use anyhow::{
    Context,
    Error,
};
use tokio::{
    io::{
        AsyncReadExt,
        AsyncWriteExt,
    },
    net::UnixStream,
};

use crate::Request;

pub struct AsyncConnection {
    stream: UnixStream,
}

impl AsyncConnection {
    pub async fn new(socket: &str) -> Result<Self, Error> {
        let stream = UnixStream::connect(socket)
            .await
            .context("socket creation failed")?;
        Ok(Self { stream })
    }

    pub async fn new_host_address() -> Result<Self, Error> {
        Self::new(rinit_ipc::get_host_address()).await
    }

    pub async fn send(
        &mut self,
        buf: &[u8],
    ) -> Result<(), Error> {
        self.stream.write_all(buf).await.context("write failed")?;
        self.stream
            .write_all("\n".as_bytes())
            .await
            .context("write failed")?;

        Ok(())
    }

    pub async fn send_request(
        &mut self,
        request: Request,
    ) -> Result<(), Error> {
        self.send(&serde_json::to_vec(&request).unwrap()).await
    }

    pub async fn _recv(&mut self) -> Result<String, Error> {
        let mut s = String::new();
        self.stream
            .read_to_string(&mut s)
            .await
            .context("error reading")?;

        Ok(s)
    }
}
