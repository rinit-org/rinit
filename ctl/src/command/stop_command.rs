use anyhow::{
    bail,
    ensure,
    Result,
};
use clap::Parser;
use rinit_ipc::{
    Connection,
    Message,
    Reply,
};

use crate::Config;

#[derive(Parser)]
pub struct StopCommand {
    services: Vec<String>,
}

impl StopCommand {
    pub fn run(
        self,
        _config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        let message = Message::StopServices(self.services);
        let mut conn = Connection::new_host_address()?;
        conn.send_message(message)?;
        let reply: Reply = serde_json::from_str(&conn.recv()?).unwrap();
        let res = if let Reply::Result(res) = reply {
            res
        } else {
            unreachable!()
        };
        if let Some(err) = res {
            bail!("{err}");
        }

        Ok(())
    }
}
