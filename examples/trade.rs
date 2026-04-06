//! 交易示例: 限价挂单 → 查挂单 → 撤单 → 市价开仓 → 查持仓 → 平仓

use crypto_bitget::exchange::bitget::*;

fn cid() -> String {
    format!("t{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis())
}

#[tokio::main]
async fn main() {
    let config = BitgetConfig::new(
        "your_api_key",
        "your_secret",
        "your_passphrase",
        "swap",
    );
    let rest = BitgetRestClient::new(config).unwrap();

    // 1. 限价挂单 (低于市价, 不会成交)
    println!("=== 1. 限价挂单 ===");
    let order = PlaceOrderRequest {
        symbol: "BTC_USDT".into(),
        side: "Buy".into(),
        order_type: "Limit".into(),
        amount: 0.001,
        price: Some(50000.0),  // 远低于市价
        cid: Some(cid()),
        time_in_force: "GTC".into(),
        ..Default::default()
    };
    let result = rest.place_order(&order).await;
    println!("下单: {}", result);
    let oid = result.get("Ok").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // 2. 查挂单
    println!("\n=== 2. 查挂单 ===");
    let orders = rest.get_open_orders("BTC_USDT").await;
    println!("{}", serde_json::to_string_pretty(&orders).unwrap());

    // 3. 撤单
    if !oid.is_empty() {
        println!("\n=== 3. 撤单 ===");
        let cancel = rest.cancel_order("BTC_USDT", Some(&oid), None).await;
        println!("撤单: {}", cancel);
    }

    // 4. 市价开多
    println!("\n=== 4. 市价开多 ===");
    let open = PlaceOrderRequest {
        symbol: "BTC_USDT".into(),
        side: "Buy".into(),
        order_type: "Market".into(),
        amount: 0.001,
        cid: Some(cid()),
        ..Default::default()
    };
    let r = rest.place_order(&open).await;
    println!("开仓: {}", r);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 5. 查持仓
    println!("\n=== 5. 查持仓 ===");
    let pos = rest.get_positions().await;
    println!("{}", serde_json::to_string_pretty(&pos).unwrap());

    // 6. 平仓
    println!("\n=== 6. 平仓 ===");
    let close = rest.close_position("BTC_USDT", Some("long")).await;
    println!("平仓: {}", close);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 7. 确认
    println!("\n=== 7. 确认持仓清空 ===");
    let pos2 = rest.get_positions().await;
    println!("{}", serde_json::to_string_pretty(&pos2).unwrap());
}
