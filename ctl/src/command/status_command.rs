use std::{
    cell::RefCell,
    rc::Rc,
};

use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use futures::stream::StreamExt;
use itertools::Itertools;
use rinit_ipc::{
    AsyncConnection,
    Reply,
    Request,
    RequestError,
};

use crate::Config;

#[derive(Parser)]
pub struct StatusCommand {
    services: Vec<String>,
}

impl StatusCommand {
    pub async fn run(
        self,
        _config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        let states = if self.services.is_empty() {
            let mut conn = AsyncConnection::new_host_address().await?;
            let request = Request::ServicesStatus();
            let res: Result<Reply, RequestError> = conn.send_request(request).await?;
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
            let conn = Rc::new(RefCell::new(AsyncConnection::new_host_address().await?));
            futures::stream::iter(
                self.services
                    .into_iter()
                    .map(|service| (service, conn.clone())),
            )
            .filter_map(async move |(service, conn)| {
                let request = Request::ServiceStatus(service);
                match conn.borrow_mut().send_request(request).await {
                    Ok(res) => {
                        match res {
                            Ok(reply) => Some(reply),
                            Err(err) => {
                                eprintln!("{err}");
                                None
                            }
                        }
                    }
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
            .await
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
