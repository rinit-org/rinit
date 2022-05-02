use anyhow::Result;
use clap::Parser;

use crate::Config;

#[derive(Parser)]
pub struct ServiceControlCommand;

impl ServiceControlCommand {
    pub async fn run(
        self,
        config: Config,
    ) -> Result<()> {
        rinit_supervision::service_control(config).await
    }
}
