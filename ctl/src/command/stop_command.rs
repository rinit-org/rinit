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
use rinit_ipc::{
    AsyncConnection,
    Reply,
    Request,
};
use rinit_service::types::RunLevel;

use crate::Dirs;

#[derive(Parser)]
pub struct StopCommand {
    #[clap(long, default_value_t)]
    runlevel: RunLevel,
    services: Vec<String>,
}

impl StopCommand {
    pub async fn run(
        self,
        _config: Dirs,
    ) -> Result<()> {
        // TODO: Print duplicated service
        ensure!(
            !(1..self.services.len()).any(|i| self.services[i..].contains(&self.services[i - 1])),
            "duplicated service found"
        );

        let conn = Rc::new(RefCell::new(AsyncConnection::new_host_address().await?));
        let success = futures::stream::iter(
            self.services
                .into_iter()
                .map(|service| (service, conn.clone())),
        )
        .map(async move |(service, conn)| -> Result<()> {
            let request = Request::StopService {
                service: service.clone(),
                runlevel: self.runlevel,
            };
            let res = conn.borrow_mut().send_request(request).await?;

            match res {
                Ok(reply) => {
                    match reply {
                        Reply::Success(success) => {
                            if success {
                                println!("Service {service} stopped successfully.");
                            } else {
                                println!("Service {service} failed to stop.");
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
        })
        .any(async move |res| res.await.is_err())
        .await;

        ensure!(success, "");

        Ok(())
    }
}
