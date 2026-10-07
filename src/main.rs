mod config;
mod db;
mod http;
mod mcp;
mod stdio;

use clap::Parser;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "c2c-server", about = "C2C MCP Message Server")]
struct Cli {
    /// Transport mode: stdio or http
    #[arg(long, default_value = "stdio")]
    mode: String,

    /// Path to config file (TOML)
    #[arg(long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = config::load(cli.config.as_deref())?;
    let db = Arc::new(db::Db::open(&config.server.db_path)?);

    match cli.mode.as_str() {
        "stdio" => {
            tokio::task::spawn_blocking(move || stdio::run(db)).await?;
        }
        "http" => http::run(db, &config).await?,
        other => anyhow::bail!("Unknown mode: {other}. Use 'stdio' or 'http'."),
    }

    Ok(())
}
