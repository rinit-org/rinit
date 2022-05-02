use std::env;

use anyhow::Result;
use rinit_ipc::Message;
use rinit_service::types::Oneshot;
use tokio::fs;

use crate::{
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};

pub async fn supervise_short_lived_process() -> Result<()> {
    let mut args = env::args();
    // Skip argv[0]
    args.next();
    let start = args.next().unwrap() == "start";
    let oneshot: Oneshot = serde_json::from_slice(&fs::read(args.next().unwrap()).await?)?;
    let success = run_short_lived_script(
        if start {
            &oneshot.start
        } else {
            &oneshot.stop.as_ref().unwrap()
        },
        signal_wait_fun(),
    )
    .await?;

    let message = Message::ServiceIsUp(if start { success } else { false }, oneshot.name);
    // TODO: log this
    message.send().await.unwrap();

    Ok(())
}
