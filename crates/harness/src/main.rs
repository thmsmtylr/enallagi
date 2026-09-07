use clap::Parser;
use harness::cli::Cli;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let code = harness::cli::run(cli)?;
    std::process::exit(code);
}
