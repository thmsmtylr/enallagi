//! The binary. Refuses a retired environment variable and the retired `run <n>` form before clap sees either, then exits with the code `cli::run` returns.

use clap::Parser;
use enallagi::cli::Cli;

fn main() -> anyhow::Result<()> {
    if let Some(message) = enallagi::config::legacy_env() {
        eprintln!("{message}");
        std::process::exit(2);
    }
    // clap has no positional here any more; catch the old `run <n>` form to name its replacement
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(String::as_str) == Some("run") {
        if let Some(n) = raw.get(2).filter(|a| a.parse::<u32>().is_ok()) {
            eprintln!("enallagi run: a bare iteration count is no longer accepted; use --iterations {n} (or -n {n})");
            std::process::exit(2);
        }
    }
    let cli = Cli::parse();
    let code = enallagi::cli::run(cli)?;
    std::process::exit(code);
}
