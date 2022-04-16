mod command;

use anyhow::Result;
use clap::Parser;
use command::{
    DisableCommand,
    EnableCommand,
    StartCommand,
    StatusCommand,
    StopCommand,
};
use kansei_core::config::Config;

#[derive(Parser)]
enum Command {
    Enable(EnableCommand),
    Disable(DisableCommand),
    Status(StatusCommand),
    Start(StartCommand),
    Stop(StopCommand),
}

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: Command,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();
    let config = Config::new(None)?;

    match opts.subcmd {
        Command::Enable(enable_command) => enable_command.run(config).await?,
        Command::Disable(disable_command) => disable_command.run(config).await?,
        Command::Status(status_command) => status_command.run(config).await?,
        Command::Start(start_command) => start_command.run(config).await?,
        Command::Stop(stop_command) => stop_command.run(config).await?,
    }

    Ok(())
}
