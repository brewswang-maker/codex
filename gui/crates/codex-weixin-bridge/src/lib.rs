//! codex-weixin-bridge: 把微信 iLink 机器人通道桥接到本机 codex app-server。
//!
//! D1 提供协议层（`wechat`）与登录 CLI；D2 起提供会话桥（`codex`）与
//! 守护进程编排。

pub mod cli;
pub mod codex;
pub mod error;
pub mod wechat;
