use anyhow::{
    bail,
    ensure,
    Result,
};
use clap::Parser;
use kansei_message::{
    Message,
    Reply,
};

use crate::Config;

#[derive(Parser)]
pub struct StartCommand {
    services: Vec<String>,
}

impl StartCommand {
    pub async fn run(
        self,
        _config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        let message = Message::StartServices(self.services);
        let reply: Reply = serde_json::from_slice(&message.send().await?).unwrap();
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
