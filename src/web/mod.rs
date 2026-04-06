//! Web 监控面板
//!
//! 功能:
//!   - 登录认证 (SHA256 密码哈希 + Token)
//!   - 实时日志推送 (WebSocket)
//!   - 策略统计、持仓、控制按钮
//!   - 暗色主题 Dashboard

pub mod server;
pub mod html;

pub use server::{WebServer, WebState, LogEntry};
