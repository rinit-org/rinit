use anyhow::Result;
use clap::Parser;
use rinit_ipc::{
    AsyncConnection,
    Request,
};

use crate::Dirs;

#[derive(Parser)]
pub struct ReloadCommand {}

impl ReloadCommand {
    pub async fn run(
        self,
        _config: Dirs,
    ) -> Result<()> {
        let mut conn = AsyncConnection::new_host_address().await?;
        conn.send_request(Request::ReloadGraph).await??;

        Ok(())
    }
}
