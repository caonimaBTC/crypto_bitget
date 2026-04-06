use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use anyhow::Result;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::signing;
use super::types::*;

// ==================== WebSocket 事件 ====================

/// Bitget WebSocket 推送事件
#[derive(Debug, Clone)]
pub enum BitgetWsEvent {
    // 行情 (公共频道)
    Ticker { symbol: String, data: Value },
    Depth { symbol: String, data: Value },
    Trade { symbol: String, data: Value },
    Kline { symbol: String, data: Value },
    FundingRate { symbol: String, data: Value },

    // 私有频道
    Order { data: Value },
    Position { data: Value },
    Balance { data: Value },

    // 连接状态
    Connected { channel: String },    // "public" | "private"
    Disconnected { channel: String },
    Error { msg: String },
}

// ==================== WebSocket 客户端 ====================

/// Bitget v2 WebSocket 客户端
///
/// 行情数据全部通过 WebSocket 实时推送:
/// - 公共频道: ticker, depth(books5/books15), trade, kline, funding-rate
/// - 私有频道: orders, positions, account (需要登录认证)
pub struct BitgetWsClient {
    pub config: BitgetConfig,
    running: Arc<AtomicBool>,
}

impl BitgetWsClient {
    pub fn new(config: BitgetConfig) -> Self {
        BitgetWsClient {
            config,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    // ==================== 公共频道连接 ====================

    /// 连接公共 WebSocket 并订阅行情
    ///
    /// subscribe_args: Bitget 订阅参数列表
    /// 例: [{"instType":"USDT-FUTURES","channel":"ticker","instId":"BTCUSDT"}]
    pub async fn connect_public(
        &self,
        subscribe_args: Vec<Value>,
        tx: mpsc::UnboundedSender<BitgetWsEvent>,
    ) -> Result<()> {
        let url = self.config.get_ws_public_url().to_string();
        let sub_msg = json!({"op": "subscribe", "args": subscribe_args});
        let running = self.running.clone();

        loop {
            if !running.load(Ordering::Relaxed) { break; }

            match connect_async(&url).await {
                Ok((ws, _)) => {
                    let _ = tx.send(BitgetWsEvent::Connected { channel: "public".into() });
                    let (mut write, mut read) = ws.split();

                    // 发送订阅消息
                    let _ = write.send(Message::Text(sub_msg.to_string())).await;

                    // ping 定时器 (25秒一次保活)
                    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(25));
                    ping_interval.tick().await; // 跳过第一次

                    loop {
                        if !running.load(Ordering::Relaxed) { break; }

                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        // Bitget 的 pong 响应是 "pong"
                                        if text == "pong" { continue; }
                                        if let Ok(data) = serde_json::from_str::<Value>(&text) {
                                            parse_public_message(&data, &tx);
                                        }
                                    }
                                    Some(Ok(Message::Ping(d))) => {
                                        let _ = write.send(Message::Pong(d)).await;
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    Some(Err(_)) => break,
                                    _ => {}
                                }
                            }
                            _ = ping_interval.tick() => {
                                // Bitget WS 需要定期发 "ping" 文本保活
                                let _ = write.send(Message::Text("ping".into())).await;
                            }
                        }
                    }

                    let _ = tx.send(BitgetWsEvent::Disconnected { channel: "public".into() });
                }
                Err(e) => {
                    let _ = tx.send(BitgetWsEvent::Error { msg: format!("公共WS连接失败: {}", e) });
                }
            }

            if !running.load(Ordering::Relaxed) { break; }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
        Ok(())
    }

    // ==================== 私有频道连接 ====================

    /// 连接私有 WebSocket (需要登录 + 订阅)
    ///
    /// subscribe_args: 私有频道订阅参数
    /// 例: [{"instType":"USDT-FUTURES","channel":"orders","instId":"default"}]
    pub async fn connect_private(
        &self,
        subscribe_args: Vec<Value>,
        tx: mpsc::UnboundedSender<BitgetWsEvent>,
    ) -> Result<()> {
        if !self.config.has_credentials() {
            return Err(anyhow::anyhow!("私有频道需要 API 凭证"));
        }

        let url = self.config.get_ws_private_url().to_string();
        let running = self.running.clone();
        let sub_msg = json!({"op": "subscribe", "args": subscribe_args});

        loop {
            if !running.load(Ordering::Relaxed) { break; }

            match connect_async(&url).await {
                Ok((ws, _)) => {
                    let (mut write, mut read) = ws.split();

                    // 每次重连都生成新的 login 消息 (timestamp + sign 必须是新的)
                    let ts = (timestamp_ms() / 1000).to_string();
                    let sign = signing::sign_ws_login(&self.config.secret, &ts);
                    let login_msg = json!({
                        "op": "login",
                        "args": [{
                            "apiKey": self.config.api_key,
                            "passphrase": self.config.passphrase,
                            "timestamp": ts,
                            "sign": sign,
                        }]
                    });

                    // Step 1: 发送 login
                    let _ = write.send(Message::Text(login_msg.to_string())).await;

                    // Step 2: 等待 login 确认响应 (最长 5 秒)
                    let mut login_ok = false;
                    let login_timeout = tokio::time::sleep(tokio::time::Duration::from_secs(5));
                    tokio::pin!(login_timeout);
                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if text == "pong" { continue; }
                                        if let Ok(data) = serde_json::from_str::<Value>(&text) {
                                            if data.get("event").and_then(|v| v.as_str()) == Some("login") {
                                                if ps(&data, "code") == "0" {
                                                    login_ok = true;
                                                } else {
                                                    let _ = tx.send(BitgetWsEvent::Error {
                                                        msg: format!("登录失败: {}", data)
                                                    });
                                                }
                                                break;
                                            }
                                            // 非 login 消息: 忽略继续等 login
                                        }
                                    }
                                    Some(Ok(Message::Ping(d))) => { let _ = write.send(Message::Pong(d)).await; }
                                    // 其他消息类型继续等待
                                    Some(Ok(_)) => {}
                                    // 连接关闭或错误才 break
                                    Some(Err(_)) | None => break,
                                }
                            }
                            _ = &mut login_timeout => {
                                let _ = tx.send(BitgetWsEvent::Error { msg: "登录超时".into() });
                                break;
                            }
                        }
                    }

                    if !login_ok { continue; } // 登录失败, 重连

                    // Step 3: 登录成功, 发送 subscribe
                    let _ = write.send(Message::Text(sub_msg.to_string())).await;
                    let _ = tx.send(BitgetWsEvent::Connected { channel: "private".into() });

                    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(25));
                    ping_interval.tick().await;

                    loop {
                        if !running.load(Ordering::Relaxed) { break; }

                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(Message::Text(text))) => {
                                        if text == "pong" { continue; }
                                        if let Ok(data) = serde_json::from_str::<Value>(&text) {
                                            // 跳过订阅确认
                                            if data.get("event").is_some() { continue; }
                                            parse_private_message(&data, &tx);
                                        }
                                    }
                                    Some(Ok(Message::Ping(d))) => {
                                        let _ = write.send(Message::Pong(d)).await;
                                    }
                                    Some(Ok(Message::Close(_))) | None => break,
                                    Some(Err(_)) => break,
                                    _ => {}
                                }
                            }
                            _ = ping_interval.tick() => {
                                let _ = write.send(Message::Text("ping".into())).await;
                            }
                        }
                    }

                    let _ = tx.send(BitgetWsEvent::Disconnected { channel: "private".into() });
                }
                Err(e) => {
                    let _ = tx.send(BitgetWsEvent::Error { msg: format!("私有WS连接失败: {}", e) });
                }
            }

            if !running.load(Ordering::Relaxed) { break; }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
        Ok(())
    }
}

// ==================== 公共频道消息解析 ====================

fn parse_public_message(data: &Value, tx: &mpsc::UnboundedSender<BitgetWsEvent>) {
    // 订阅确认/错误消息跳过
    if data.get("event").is_some() { return; }

    let empty = json!({});
    let arg = data.get("arg").unwrap_or(&empty);
    let channel = ps(arg, "channel");

    let items = match data.get("data").and_then(|d| d.as_array()) {
        Some(arr) => arr,
        None => return,
    };

    for d in items {
        // 优先从 data 取 instId/symbol，fallback 到 arg.instId
        let inst_raw = if !ps(d, "instId").is_empty() {
            ps(d, "instId").to_string()
        } else if !ps(d, "symbol").is_empty() {
            ps(d, "symbol").to_string()
        } else {
            ps(arg, "instId").to_string()
        };
        let sym = from_bitget_symbol(&inst_raw);

        match channel {
            // ===== Ticker (BBO) =====
            "ticker" => {
                let _ = tx.send(BitgetWsEvent::Ticker {
                    symbol: sym.clone(),
                    data: json!({
                        "symbol": sym,
                        "bid_price": pf(d, "bestBid"),
                        "bid_size": pf(d, "bestBidSz"),
                        "ask_price": pf(d, "bestAsk"),
                        "ask_size": pf(d, "bestAskSz"),
                        "last_price": pf(d, "lastPr"),
                        "mark_price": pf(d, "markPx"),
                        "index_price": pf(d, "indexPx"),
                        "funding_rate": pf(d, "fundingRate"),
                        "high_24h": pf(d, "high24h"),
                        "low_24h": pf(d, "low24h"),
                        "volume_24h": pf(d, "baseVolume"),
                        "timestamp": pf(d, "ts") as u64,
                    }),
                });
            }

            // ===== 深度 =====
            "books5" | "books15" | "books" => {
                let _ = tx.send(BitgetWsEvent::Depth {
                    symbol: sym.clone(),
                    data: json!({
                        "symbol": sym,
                        "bids": d.get("bids").cloned().unwrap_or(json!([])),
                        "asks": d.get("asks").cloned().unwrap_or(json!([])),
                        "timestamp": pf(d, "ts") as u64,
                    }),
                });
            }

            // ===== 成交 =====
            "trade" | "trades" => {
                let price = { let a = pf(d, "px"); if a > 0.0 { a } else { pf(d, "price") } };
                let amount = { let a = pf(d, "sz"); if a > 0.0 { a } else { pf(d, "size") } };
                let _ = tx.send(BitgetWsEvent::Trade {
                    symbol: sym.clone(),
                    data: json!({
                        "symbol": sym,
                        "price": price,
                        "amount": amount,
                        "side": if ps(d, "side") == "buy" { "Buy" } else { "Sell" },
                        "timestamp": pf(d, "ts") as u64,
                    }),
                });
            }

            // ===== K线 =====
            ch if ch.starts_with("candle") => {
                // Bitget candle 数据格式: [ts, open, high, low, close, volume, ...]
                if let Some(arr) = d.as_array() {
                    if arr.len() >= 6 {
                        let _ = tx.send(BitgetWsEvent::Kline {
                            symbol: sym.clone(),
                            data: json!({
                                "symbol": sym,
                                "interval": ch.replace("candle", ""),
                                "timestamp": arr[0].as_str().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
                                "open": arr[1].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                                "high": arr[2].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                                "low": arr[3].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                                "close": arr[4].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                                "volume": arr[5].as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                            }),
                        });
                    }
                }
            }

            // ===== 资金费率 =====
            "funding-rate" => {
                let _ = tx.send(BitgetWsEvent::FundingRate {
                    symbol: sym.clone(),
                    data: json!({
                        "symbol": sym,
                        "funding_rate": pf(d, "fundingRate"),
                        "next_funding_time": pf(d, "nextFundingTime") as u64,
                    }),
                });
            }

            _ => {}
        }
    }
}

// ==================== 私有频道消息解析 ====================

fn parse_private_message(data: &Value, tx: &mpsc::UnboundedSender<BitgetWsEvent>) {
    // 订阅确认/login 响应跳过
    if data.get("event").is_some() { return; }

    let empty = json!({});
    let arg = data.get("arg").unwrap_or(&empty);
    let channel = ps(arg, "channel");

    let items = match data.get("data").and_then(|d| d.as_array()) {
        Some(arr) => arr,
        None => return,
    };

    for d in items {
        match channel {
            // ===== 订单推送 =====
            "orders" => {
                let inst_raw = if !ps(d, "instId").is_empty() {
                    ps(d, "instId").to_string()
                } else if !ps(d, "symbol").is_empty() {
                    ps(d, "symbol").to_string()
                } else {
                    ps(arg, "instId").to_string()
                };
                let sym = from_bitget_symbol(&inst_raw);

                let status = match ps(d, "status") {
                    "new" | "live" | "init" => "Open",
                    "partial-fill" | "partially_filled" => "PartiallyFilled",
                    "full-fill" | "filled" => "Filled",
                    "cancelled" | "canceled" => "Canceled",
                    other => {
                        eprintln!("[Bitget] 未知订单状态: {}", other);
                        other // 保留原始值，不静默转为 Canceled
                    }
                };

                // 兼容 v1/v2 字段名
                let ord_id = {
                    let a = ps(d, "ordId"); let b = ps(d, "orderId");
                    if !a.is_empty() { a } else { b }
                };
                let cl_id = {
                    let a = ps(d, "clOrdId"); let b = ps(d, "clientOid");
                    if !a.is_empty() { a } else { b }
                };

                let price = { let a = pf(d, "px"); if a > 0.0 { a } else { pf(d, "price") } };
                let amount = { let a = pf(d, "sz"); if a > 0.0 { a } else { pf(d, "size") } };
                let filled = { let a = pf(d, "fillSz"); if a > 0.0 { a } else { pf(d, "baseVolume") } };
                let avg_price = { let a = pf(d, "avgPx"); if a > 0.0 { a } else { pf(d, "priceAvg") } };

                let order = json!({
                    "id": ord_id,
                    "cid": cl_id,
                    "symbol": sym,
                    "side": if ps(d, "side") == "buy" { "Buy" } else { "Sell" },
                    "order_type": ps(d, "ordType"),
                    "price": price,
                    "amount": amount,
                    "filled": filled,
                    "avg_price": avg_price,
                    "status": status,
                    "trade_side": ps(d, "tradeSide"),
                    "fee": pf(d, "fee"),
                });

                let _ = tx.send(BitgetWsEvent::Order { data: order });
            }

            // ===== 持仓推送 =====
            "positions" => {
                let inst_raw = if !ps(d, "instId").is_empty() {
                    ps(d, "instId").to_string()
                } else {
                    ps(d, "symbol").to_string()
                };
                let sym = from_bitget_symbol(&inst_raw);

                let amt = pf(d, "total");
                let hold_side = ps(d, "holdSide").to_lowercase();
                let position = json!({
                    "symbol": sym,
                    "side": match hold_side.as_str() {
                        "long" => "Long", "short" => "Short", _ => ps(d, "holdSide")
                    },
                    "amount": amt.abs(),
                    "entry_price": pf(d, "openPriceAvg"),
                    "mark_price": pf(d, "markPrice"),
                    "unrealized_pnl": pf(d, "unrealizedPL"),
                    "leverage": pf(d, "leverage"),
                    "margin_mode": ps(d, "marginMode"),
                });

                let _ = tx.send(BitgetWsEvent::Position { data: json!([position]) });
            }

            // ===== 账户余额推送 =====
            "account" => {
                let balance = json!({
                    "asset": ps(d, "marginCoin"),
                    "balance": pf(d, "equity"),
                    "available_balance": pf(d, "available"),
                    "frozen_balance": pf(d, "locked"),
                });

                let _ = tx.send(BitgetWsEvent::Balance { data: json!([balance]) });
            }

            _ => {}
        }
    }
}

// ==================== 订阅参数构造辅助 ====================

/// 构造 ticker 订阅参数
pub fn sub_ticker(inst_type: &str, symbol: &str) -> Value {
    json!({"instType": inst_type, "channel": "ticker", "instId": to_bitget_symbol(symbol)})
}

/// 构造 depth 订阅参数
pub fn sub_depth(inst_type: &str, symbol: &str, level: &str) -> Value {
    json!({"instType": inst_type, "channel": level, "instId": to_bitget_symbol(symbol)})
}

/// 构造 trade 订阅参数
pub fn sub_trade(inst_type: &str, symbol: &str) -> Value {
    json!({"instType": inst_type, "channel": "trade", "instId": to_bitget_symbol(symbol)})
}

/// 构造 K线 订阅参数
pub fn sub_kline(inst_type: &str, symbol: &str, interval: &str) -> Value {
    let bg_interval = match interval.to_lowercase().as_str() {
        "1m" => "1m", "5m" => "5m", "15m" => "15m", "30m" => "30m",
        "1h" => "1H", "4h" => "4H", "1d" | "1day" => "1D", "1w" | "1week" => "1W",
        _ => interval, // 未知 interval 直接传递，不静默降级
    };
    json!({"instType": inst_type, "channel": format!("candle{}", bg_interval), "instId": to_bitget_symbol(symbol)})
}

/// 构造 funding-rate 订阅参数
pub fn sub_funding_rate(symbol: &str) -> Value {
    json!({"instType": "USDT-FUTURES", "channel": "funding-rate", "instId": to_bitget_symbol(symbol)})
}

/// 构造 orders 私有订阅参数
pub fn sub_orders(inst_type: &str) -> Value {
    json!({"instType": inst_type, "channel": "orders", "instId": "default"})
}

/// 构造 positions 私有订阅参数
pub fn sub_positions(inst_type: &str) -> Value {
    json!({"instType": inst_type, "channel": "positions", "instId": "default"})
}

/// 构造 account 私有订阅参数
pub fn sub_account(inst_type: &str) -> Value {
    json!({"instType": inst_type, "channel": "account", "coin": "default"})
}
