//! Error type shared across the weixin bridge.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),

    #[error("响应解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("iLink API 错误 ret={ret} errcode={errcode:?} errmsg={errmsg:?}")]
    Api {
        ret: i64,
        errcode: Option<i64>,
        errmsg: Option<String>,
    },

    #[error("未登录：请先运行 `codex-weixin-bridge login` 扫码连接微信")]
    NotLoggedIn,

    #[error("文件读写失败: {0}")]
    Io(#[from] std::io::Error),

    #[error("凭据文件损坏或不可读: {0}")]
    Credentials(String),

    #[error("状态目录无法创建: {path}: {source}")]
    StateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("登录失败: {0}")]
    Login(String),

    #[error("codex app-server 错误: {0}")]
    AppServer(#[from] codex_gui_bridge::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, BridgeError>;
