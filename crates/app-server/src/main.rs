use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use deepseek_app_server::{AppServerOptions, run};

#[derive(Debug, Parser)]
#[command(
    name = "deepseek-app-server",
    about = "运行 DeepSeek 应用服务器传输层"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8787)]
    port: u16,
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let listen: SocketAddr = format!("{}:{}", cli.host, cli.port)
        .parse()
        .with_context(|| format!("无效的监听地址 {}:{}", cli.host, cli.port))?;
    run(AppServerOptions {
        listen,
        config_path: cli.config,
    })
    .await
}
