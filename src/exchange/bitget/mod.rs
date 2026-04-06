//! Bitget v2 交易所完整框架
//!
//! 架构:
//!   - types.rs        数据类型、常量、符号转换
//!   - signing.rs      HMAC-SHA256 Base64 签名 (REST + WS)
//!   - rest.rs         REST API 客户端 (账户、交易、合约设置)
//!   - ws.rs           WebSocket 客户端 (行情 + 私有频道, 含消息解析)
//!   - subscription.rs 订阅消息构造器 (策略配置 -> WS 订阅参数)
//!
//! 行情数据全部通过 WebSocket 实时推送，REST 仅处理交易和账户操作。
//!
//! 使用示例:
//! ```rust,no_run
//! use crate::exchange::bitget::{BitgetConfig, BitgetRestClient, BitgetWsClient};
//! use crate::exchange::bitget::ws;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = BitgetConfig::new("key", "secret", "passphrase", "swap");
//!
//!     // REST: 查询余额、下单
//!     let rest = BitgetRestClient::new(config.clone()).unwrap();
//!     let balance = rest.get_usdt_balance().await;
//!
//!     // WebSocket: 行情订阅
//!     let ws_client = BitgetWsClient::new(config.clone());
//!     let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
//!
//!     let pub_args = vec![
//!         ws::sub_ticker("USDT-FUTURES", "BTC_USDT"),
//!         ws::sub_depth("USDT-FUTURES", "BTC_USDT", "books5"),
//!         ws::sub_trade("USDT-FUTURES", "BTC_USDT"),
//!     ];
//!     tokio::spawn(async move {
//!         ws_client.connect_public(pub_args, tx).await.unwrap();
//!     });
//!
//!     while let Some(event) = rx.recv().await {
//!         // 处理事件...
//!     }
//! }
//! ```

pub mod types;
pub mod signing;
pub mod rest;
pub mod ws;
pub mod subscription;

// Re-exports
pub use types::{BitgetConfig, PlaceOrderRequest, Balance, Position, Order, Ticker, Instrument, Kline, Trade};
pub use types::{to_bitget_symbol, from_bitget_symbol, pf, ps, ok_result, err_result, timestamp_ms};
pub use rest::BitgetRestClient;
pub use ws::{BitgetWsClient, BitgetWsEvent};
pub use subscription::build_subscribe_messages;
