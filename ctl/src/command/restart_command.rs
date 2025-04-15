use anyhow::{
    ensure,
    Result,
};
use clap::Parser;
use rinit_ipc::AsyncConnection;
use rinit_service::{
    config::Config,
    types::RunLevel,
};

use crate::util::start_service;

#[derive(Parser)]
pub struct RestartCommand {
    #[clap(long, default_value_t)]
    runlevel: RunLevel,
    services: Vec<String>,
}

impl RestartCommand {
    pub async fn run(
        self,
        _config: Config,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        // let mut conn = AsyncConnection::new_host_address().await?;
        // let mut error = false;
        // for service in self.services {
        //     if stop_service(&mut conn, &service, self.runlevel).await? {
        //         println!("Service {service} stopped successfully.");
        //     } else {
        //         println!("Service {service} failed to stop.");
        //         error = true;
        //     }
        //     if start_service(&mut conn, &service, self.runlevel).await? {
        //         println!("Service {service} restarted successfully.");
        //     } else {
        //         println!("Service {service} failed to start.");
        //         error = true;
        //     }
        // }
        //
        // ensure!(!error, "");
        Ok(())
    }
}
