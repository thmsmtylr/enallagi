use clap::Parser;
use harness::cli::Cli;

fn main() -> anyhow::Result<()> {
    // clap has no positional here any more; catch the old `run <n>` form to name its replacement
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(String::as_str) == Some("run") {
        if let Some(n) = raw.get(2).filter(|a| a.parse::<u32>().is_ok()) {
            eprintln!("harness run: a bare iteration count is no longer accepted; use --iterations {n} (or -n {n})");
            std::process::exit(2);
        }
    }
    let cli = Cli::parse();
    let code = harness::cli::run(cli)?;
    std::process::exit(code);
}
