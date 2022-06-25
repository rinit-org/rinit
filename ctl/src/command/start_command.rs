use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use rinit_ipc::{
    request_error::RequestError,
    Connection,
    Reply,
    Request,
};

use crate::Config;

#[derive(Parser)]
pub struct StartCommand {
    services: Vec<String>,
}

impl StartCommand {
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
        self.services
            .into_iter()
            .try_for_each(|service| -> Result<()> {
                let request = Request::StartService(service.clone());
                conn.send_request(request)?;

                let res: Result<Reply, RequestError> = serde_json::from_str(&conn.recv()?).unwrap();
                match res {
                    Ok(reply) => {
                        match reply {
                            Reply::Success(success) => {
                                if success {
                                    println!("Service {service} started successfully.");
                                } else {
                                    println!("Service {service} failed to start.");
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                    Err(err) => {
                        eprintln!("{err}");
                    }
                }
                Ok(())
            })?;

        Ok(())
    }
}
