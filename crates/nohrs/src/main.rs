mod app;
mod cli;

use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();

    // `config` subcommands run headless and exit without opening a window.
    if let Some(cli::Command::Config { action }) = &cli.command {
        nohrs_core::telemetry::logging::init_logging();
        std::process::exit(cli::run_config_command(action, &cli));
    }

    app::NohrsApp::run(&cli);
}
