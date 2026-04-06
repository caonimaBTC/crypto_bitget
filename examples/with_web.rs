//! Web 面板示例: 启动监控面板 + 交易

use crypto_bitget::exchange::bitget::*;
use crypto_bitget::web::{WebServer, WebState};
use crypto_bitget::logger::Logger;
use serde_json::json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 1. 启动 Web 面板
    let web = Arc::new(WebState::new("admin", "123456"));
    Logger::bind_web_state(web.clone());
    let wc = web.clone();
    tokio::spawn(async move { WebServer::start(wc, "0.0.0.0", 8888).await; });

    let log = Logger::new("INFO", None);
    log.log("Web 面板: http://localhost:8888 (admin/123456)", "INFO", Some("green"));

    // 2. 连接交易所
    let config = BitgetConfig::new("your_key", "your_secret", "your_pass", "swap");
    let rest = BitgetRestClient::new(config).unwrap();

    // 3. 查余额并推送到面板
    let bal = rest.get_usdt_balance().await;
    let b = bal.get("Ok").and_then(|v| v.get("balance")).and_then(|v| v.as_f64()).unwrap_or(0.0);
    log.log(&format!("余额: {:.2} USDT", b), "INFO", Some("green"));

    web.update_stats(json!({
        "current_balance": b, "initial_balance": b, "available_balance": b,
        "total_profit": 0, "win_rate": 0, "count": 0,
        "server_name": "Bitget Live",
        "strategies": [{"name": "BTC策略", "symbol": "BTC_USDT", "exchange": "Bitget", "balance": b}],
    }));

    // 4. 查持仓并推送
    let pos = rest.get_positions().await;
    if let Some(ok) = pos.get("Ok") {
        web.update_positions(ok.clone());
    }

    log.log("面板已更新, 打开浏览器查看", "INFO", Some("cyan"));

    // 保持运行
    loop { tokio::time::sleep(std::time::Duration::from_secs(60)).await; }
}
