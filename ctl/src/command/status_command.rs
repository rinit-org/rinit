use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use itertools::Itertools;
use rinit_ipc::{
    request_error::RequestError,
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

        let mut conn = Connection::new_host_address()?;
        let states = if self.services.is_empty() {
            let request = Request::ServicesStatus();
            conn.send_request(request).unwrap();
            let res: Result<Reply, RequestError> =
                serde_json::from_str(&conn.recv().unwrap()).unwrap();
            match res {
                Ok(reply) => {
                    match reply {
                        Reply::ServicesStates(states) => states,
                        _ => unreachable!(),
                    }
                }
                Err(err) => {
                    eprintln!("{err}");
                    Vec::new()
                }
            }
        } else {
            self.services
                .into_iter()
                .filter_map(|service| {
                    let request = Request::ServiceStatus(service);
                    conn.send_request(request).unwrap();

                    let res: Result<Reply, RequestError> =
                        serde_json::from_str(&conn.recv().unwrap()).unwrap();
                    match res {
                        Ok(reply) => Some(reply),
                        Err(err) => {
                            eprintln!("{err}");
                            None
                        }
                    }
                })
                .map(|reply| {
                    match reply {
                        Reply::ServiceState(service, state) => (service, state),
                        _ => unreachable!(),
                    }
                })
                .collect()
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
