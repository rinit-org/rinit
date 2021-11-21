mod command;

use anyhow::{
    Context,
    Result,
};
use clap::Parser;
use command::{
    DisableCommand,
    EnableCommand,
};
use kansei_core::config::Config;

#[derive(Parser)]
enum Command {
    Enable(EnableCommand),
    Disable(DisableCommand),
}

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: Command,
}

#[async_std::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();
    let config = Config::new(None)?;

    match opts.subcmd {
        Command::Enable(enable_command) => enable_command.run(config).await?,
        Command::Disable(disable_command) => disable_command.run(config).await?,
    }

    Ok(())
}
