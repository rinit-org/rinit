use std::{
    io::prelude::*,
    os::unix::net::UnixStream,
};

use serde::{
    Deserialize,
    Serialize,
};
use snafu::{
    whatever,
    ResultExt,
    Snafu,
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
    pub fn send(self) -> Result<Vec<u8>, Error> {
        self.send_to(get_host_address())
    }

    fn send_to(
        self,
        socket: &str,
    ) -> Result<Vec<u8>, Error> {
        let mut stream = UnixStream::connect(socket)
            .with_whatever_context(|_| format!("unable to accept connection to {socket}"))?;

        let message = serde_json::to_vec(&self).whatever_context("error serializing message")?;
        match stream.write_all(&message) {
            Ok(_) => {}
            Err(e) => {
                whatever!("error sending message: {}", e);
            }
        }
        match stream.write("\n".as_bytes()) {
            Ok(_) => {}
            Err(e) => {
                whatever!("error sending message: {}", e);
            }
        }

        let mut res = Vec::new();
        loop {
            let mut buf = [0; 1024];
            match stream.read(&mut buf) {
                Ok(size) if size == 0 => break,
                Ok(_) => {
                    let index = buf.iter().position(|&c| c == 10);
                    res.extend_from_slice(&buf[..index.unwrap_or(buf.len())]);
                    if index.is_some() {
                        break;
                    }
                }
                Err(_) => {
                    todo!()
                }
            }
        }

        Ok(res)
    }
}
