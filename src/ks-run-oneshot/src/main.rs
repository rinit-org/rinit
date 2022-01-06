use std::env;

use anyhow::Result;
use kansei_core::types::Script;
use kansei_exec::{
    run_short_lived_script,
    signal_wait,
};
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let script: Script = bincode::deserialize(&fs::read(args.next().unwrap()).await?)?;
    let success = run_short_lived_script(&script, signal_wait()).await?;

    println!("success: {}", success);

    //TODO: notify svc

    Ok(())
}
