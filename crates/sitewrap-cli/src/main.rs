use anyhow::Result;
use clap::Parser;
use sitewrap_app::{AppMode, APP_ID};
use tracing::Level;
use uuid::Uuid;

/// Sitewrap command-line entrypoint.
#[derive(Parser, Debug)]
#[command(author, version, about = "Run Sitewrap manager or a specific web app shell", long_about = None)]
struct Args {
    /// Launch shell mode for the given web app id.
    #[arg(long)]
    shell: Option<Uuid>,

    /// Force manager mode even if other args are present.
    #[arg(long)]
    manager: bool,
}

fn init_tracing() {
    // Default to info unless the user sets RUST_LOG.
    let env = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("{APP_ID}=info,sitewrap=info").into());
    tracing_subscriber::fmt()
        .with_env_filter(env)
        .with_max_level(Level::INFO)
        .init();
}

fn main() -> Result<()> {
    init_tracing();
    let args = Args::parse();

    let mode = if args.manager {
        AppMode::Manager
    } else if let Some(id) = args.shell {
        AppMode::Shell(id)
    } else {
        AppMode::Manager
    };

    sitewrap_app::run(mode)
}
