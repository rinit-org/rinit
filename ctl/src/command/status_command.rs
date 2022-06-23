use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use itertools::Itertools;
use rinit_ipc::{
    Connection,
    Reply,
    Request,
};

use crate::Config;

#[derive(Parser)]
pub struct StatusCommand {
    services: Vec<String>,
}

impl StatusCommand {
    pub fn run(
        self,
        _config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        let request = Request::ServicesStatus(self.services);
        let mut conn = Connection::new_host_address()?;
        conn.send_request(request)?;
        let reply: Reply = serde_json::from_str(&conn.recv()?).unwrap();
        let states = if let Reply::ServicesStates(states) = reply {
            states
        } else {
            unreachable!()
        };
        states
            .iter()
            .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
            .for_each(|state| {
                // TODO: Add better formatting
                println!("{}: {}", state.0, state.1);
            });

        Ok(())
    }
}
