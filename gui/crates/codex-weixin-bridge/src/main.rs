//! `codex-weixin-bridge` binary entrypoint.

use clap::Parser as _;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = codex_weixin_bridge::cli::Cli::parse();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("错误: 无法创建 tokio runtime: {err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = runtime.block_on(codex_weixin_bridge::cli::run(cli)) {
        eprintln!("错误: {err}");
        std::process::exit(1);
    }
}
