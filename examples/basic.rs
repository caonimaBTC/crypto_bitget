//! 基础示例: 查余额、查持仓、查价格

use crypto_bitget::exchange::bitget::*;

#[tokio::main]
async fn main() {
    let config = BitgetConfig::new(
        "your_api_key",
        "your_secret",
        "your_passphrase",
        "swap",  // "swap" 合约 | "spot" 现货
    );

    let rest = BitgetRestClient::new(config).unwrap();

    // 查余额
    println!("=== 余额 ===");
    let bal = rest.get_usdt_balance().await;
    println!("{}", serde_json::to_string_pretty(&bal).unwrap());

    // 查持仓
    println!("\n=== 持仓 ===");
    let pos = rest.get_positions().await;
    println!("{}", serde_json::to_string_pretty(&pos).unwrap());

    // 查 BTC 价格
    println!("\n=== BTC 价格 ===");
    let ticker = rest.request_raw("GET", "/api/v2/mix/market/ticker", false,
        Some(&serde_json::json!({"productType":"USDT-FUTURES","symbol":"BTCUSDT"})), None).await;
    println!("{}", serde_json::to_string_pretty(&ticker).unwrap());
}
