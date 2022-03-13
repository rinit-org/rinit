use std::env;

use anyhow::Result;
use kansei_core::types::Oneshot;
use kansei_exec::{
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};
use kansei_message::Message;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
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
