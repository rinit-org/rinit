use std::env;

use anyhow::Result;
use kansei_core::types::Script;
use kansei_exec::{
    run_short_lived_script,
    signal_wait::signal_wait_fun,
};
use kansei_message::Message;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let script: Script = serde_json::from_slice(&mut fs::read(args.next().unwrap()).await?)?;
    let success = run_short_lived_script(&script, signal_wait_fun()).await?;

    println!("success: {}", success);

    //TODO: notify svc
    let message = Message::ServiceIsUp(true, "myser".to_string());
    // TODO: log this
    message.send().await.unwrap();

    Ok(())
}
