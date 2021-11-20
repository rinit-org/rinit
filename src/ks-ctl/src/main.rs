mod command;

use anyhow::{
    Context,
    Result,
};
use clap::Parser;
use command::EnableCommand;
use kansei_core::config::Config;

#[derive(Parser)]
enum Command {
    Enable(EnableCommand),
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
    }

    Ok(())
}
