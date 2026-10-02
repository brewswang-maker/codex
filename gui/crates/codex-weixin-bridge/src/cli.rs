//! Command-line surface: `codex-weixin-bridge login|status|run`.

use std::time::Duration;

use clap::{Parser, Subcommand};

use crate::error::Result;
use crate::wechat::auth::{
    AccountData, LoginEvent, LoginOutcome, Store, render_qr_to_terminal, run_login,
};
use crate::wechat::client::WeixinClient;
use crate::wechat::types::DEFAULT_BASE_URL;

/// Interactive login ceiling (mirrors the JS channel's 8 minutes).
const LOGIN_TIMEOUT: Duration = Duration::from_secs(480);

#[derive(Parser)]
#[command(
    name = "codex-weixin-bridge",
    about = "把微信 iLink 机器人通道桥接到本机 codex app-server"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// 扫码登录微信并把凭据保存到 ~/.codex/weixin-bridge/
    Login,
    /// 查看当前登录状态
    Status,
    /// 启动桥接守护进程（微信 <-> codex app-server）
    Run,
}

/// Dispatch a parsed CLI command; errors carry user-facing Chinese messages.
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Login => cmd_login().await,
        Command::Status => cmd_status(),
        Command::Run => cmd_run(),
    }
}

async fn cmd_login() -> Result<()> {
    let store = Store::with_default_home()?;
    let client = WeixinClient::new(DEFAULT_BASE_URL, None)?;
    println!("正在启动，即将获取登录二维码…");

    let outcome = run_login(
        &client,
        LOGIN_TIMEOUT,
        |event| match event {
            LoginEvent::QrReady { url } | LoginEvent::QrRefreshed { url, .. } => {
                render_qr_to_terminal(&url);
                println!("请用手机微信扫描以上二维码，以继续连接：");
            }
            LoginEvent::Scanned => println!("已扫码，请在手机上确认。"),
            LoginEvent::NeedVerifyCode => print!("输入手机微信显示的数字，以继续连接："),
            LoginEvent::Message(text) => println!("{text}"),
            LoginEvent::Waiting => {}
        },
        read_verify_code_from_stdin,
    )
    .await;

    match outcome {
        LoginOutcome::Connected(account) => {
            store.save_account(&account)?;
            println!("已连接微信。");
            println!("  bot id: {}", account.account_id);
            if let Some(user_id) = &account.user_id {
                println!("  扫码用户: {user_id}");
            }
            println!("凭据已保存，可运行 `codex-weixin-bridge run` 启动桥接。");
            Ok(())
        }
        LoginOutcome::AlreadyConnected => {
            println!("此机器人已连接过，本地凭据保持有效，无需重复登录。");
            Ok(())
        }
        LoginOutcome::Failed(message) => Err(crate::error::BridgeError::Login(message)),
    }
}

fn cmd_status() -> Result<()> {
    let store = Store::with_default_home()?;
    match store.load_account() {
        Some(AccountData {
            account_id,
            user_id,
            base_url,
            saved_at_ms,
            ..
        }) => {
            println!("已登录");
            println!("  bot id: {account_id}");
            if let Some(user_id) = &user_id {
                println!("  扫码用户: {user_id}");
            }
            println!(
                "  接入点: {}",
                base_url.as_deref().unwrap_or(DEFAULT_BASE_URL)
            );
            println!("  保存时间: {saved_at_ms} (unix ms)");
            println!("  同步游标: {} 字节", store.load_sync_buf().len());
        }
        None => {
            println!("未登录。请先运行 `codex-weixin-bridge login` 扫码连接微信。");
        }
    }
    Ok(())
}

fn cmd_run() -> Result<()> {
    // 会话桥在 D2 提供（codex/session.rs + runtime.rs）；当前版本先拒绝启动，
    // 避免静默空跑。
    Err(crate::error::BridgeError::Other(
        "run 子命令将在 D2（会话桥）落地后可用；当前版本支持 login / status。".to_string(),
    ))
}

async fn read_verify_code_from_stdin() -> String {
    use tokio::io::AsyncBufReadExt as _;
    let mut line = String::new();
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    match reader.read_line(&mut line).await {
        Ok(_) => line.trim().to_string(),
        Err(_) => String::new(),
    }
}
