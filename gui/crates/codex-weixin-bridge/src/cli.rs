//! Command-line surface: `codex-weixin-bridge login|status|run|allow|send`.

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
    Run {
        /// codex 后端程序路径；缺省从 PATH 解析 `codex`
        #[arg(long = "app-server", value_name = "PROGRAM")]
        app_server: Option<std::path::PathBuf>,
    },
    /// 放行一个联系人（加入白名单）
    Allow {
        /// 联系人用户 id（微信 iLink 用户标识）
        contact: String,
    },
    /// 主动向联系人发送一条文本消息（走完整发送礼仪，用于链路自检）
    Send {
        /// 目标联系人用户 id
        contact: String,
        /// 消息文本
        text: String,
    },
}

/// Dispatch a parsed CLI command; errors carry user-facing Chinese messages.
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Login => cmd_login().await,
        Command::Status => cmd_status(),
        Command::Run { app_server } => cmd_run(app_server).await,
        Command::Allow { contact } => cmd_allow(&contact),
        Command::Send { contact, text } => cmd_send(&contact, &text).await,
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

async fn cmd_run(app_server: Option<std::path::PathBuf>) -> Result<()> {
    let store = Store::with_default_home()?;
    if store.load_account().is_none() {
        return Err(crate::error::BridgeError::NotLoggedIn);
    }
    if let Err(err) = crate::codex::runtime::run(store, app_server).await {
        // errcode -14 is the server's session-timeout: the daemon stops and
        // the user must scan again (登录过期自动重扫提示).
        if matches!(
            &err,
            crate::error::BridgeError::Api {
                errcode: Some(-14),
                ..
            }
        ) {
            return Err(crate::error::BridgeError::Login(format!(
                "{err}；登录已过期，请重新运行 `codex-weixin-bridge login` 扫码连接微信"
            )));
        }
        return Err(err);
    }
    Ok(())
}

/// Adds a contact to the persistent allowlist so it may drive codex tasks.
fn cmd_allow(contact: &str) -> Result<()> {
    let store = Store::with_default_home()?;
    let contact = crate::codex::control::sanitize_contact(contact)?;
    let mut allowlist = crate::codex::allowlist::AllowList::load(store.codex_home(), None);
    if allowlist.grant(&contact)? {
        println!("已放行联系人: {contact}");
    } else {
        println!("联系人已在白名单中: {contact}");
    }
    Ok(())
}

/// Proactively sends a text message through the same preparation etiquette
/// (`getconfig` ticket -> typing -> sendmessage -> cancel) the daemon uses,
/// so link self-checks exercise the real outbound path.
async fn cmd_send(contact: &str, text: &str) -> Result<()> {
    let store = Store::with_default_home()?;
    let contact = crate::codex::control::sanitize_contact(contact)?;
    let client = store.load_client()?;
    let msg = crate::wechat::types::WeixinMessage::text_reply(
        &contact,
        text,
        None,
        &crate::wechat::types::generate_client_id(),
    );
    let resp = client.send_prepared(msg).await?;
    println!("已发送到 {contact}（message_id={:?}）", resp.message_id);
    Ok(())
}

fn read_verify_code_from_stdin()
-> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send>> {
    Box::pin(async {
        use tokio::io::AsyncBufReadExt as _;
        let mut line = String::new();
        let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
        match reader.read_line(&mut line).await {
            Ok(_) => line.trim().to_string(),
            Err(_) => String::new(),
        }
    })
}
