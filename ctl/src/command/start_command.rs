use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use rinit_ipc::{
    AsyncConnection,
    Reply,
    Request,
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

        let mut conn = AsyncConnection::new_host_address().await?;
        let mut error = false;
        for service in self.services {
            let request = Request::StartService(service.clone());
            let res = conn.send_request(request).await?;

            match res {
                Ok(reply) => {
                    match reply {
                        Reply::Success(success) => {
                            if success {
                                println!("Service {service} started successfully.");
                            } else {
                                println!("Service {service} failed to start.");
                                error = true;
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Err(err) => {
                    eprintln!("{err}");
                    error = true;
                }
            }
        }

        ensure!(!error, "");
        Ok(())
    }
}
