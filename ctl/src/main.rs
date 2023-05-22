#![feature(async_closure)]

mod command;
mod util;

use anyhow::Result;
use clap::Parser;
#[derive(Parser)]
enum Command {
    Enable(EnableCommand),
    Disable(DisableCommand),
    Status(StatusCommand),
    Start(StartCommand),
    Stop(StopCommand),
    Reload(ReloadCommand),
}

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: Command,
}
use command::{
    DisableCommand,
    EnableCommand,
    ReloadCommand,
    StartCommand,
    StatusCommand,
    StopCommand,
};
use rinit_service::config::Config;

// This has to be async just for AsyncConnection
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let opts = Opts::parse();
    let config = Config::new(None)?;

    match opts.subcmd {
        Command::Enable(enable_command) => enable_command.run(config).await?,
        Command::Disable(disable_command) => disable_command.run(config).await?,
        Command::Status(status_command) => status_command.run(config).await?,
        Command::Start(start_command) => start_command.run(config).await?,
        Command::Stop(stop_command) => stop_command.run(config).await?,
        Command::Reload(reload_command) => reload_command.run(config).await?,
    }

    Ok(())
}
