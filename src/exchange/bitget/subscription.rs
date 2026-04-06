use serde_json::{json, Value};
use std::collections::HashSet;

use super::types::*;
use super::signing;
use super::ws;

/// 从策略订阅配置构建 Bitget 公共/私有 WebSocket 消息
///
/// 输入: ws_subs 是策略声明的订阅列表, 格式如:
///   [{"Bbo": ["BTC_USDT", "ETH_USDT"]}, {"Order": ["BTC_USDT"]}, ...]
///
/// 输出: (公共频道(url, 消息列表), 私有频道(url, 消息列表))
///   私有频道消息列表 = [login_msg, subscribe_msg] (两步序列)
pub fn build_subscribe_messages(
    config: &BitgetConfig,
    ws_subs: &[Value],
) -> (Option<(String, Vec<Value>)>, Option<(String, Vec<Value>)>) {
    let mut pub_args = vec![];
    let mut priv_args = vec![];
    let inst_type = config.inst_type();

    for item in ws_subs {
        if let Some(obj) = item.as_object() {
            for (typ, params) in obj {
                let symbols = extract_symbols(params);
                for sym in &symbols {
                    match typ.as_str() {
                        // ===== 公共行情频道 =====
                        "Bbo" | "Ticker" => {
                            pub_args.push(ws::sub_ticker(inst_type, sym));
                        }
                        "Depth" => {
                            let level = params.as_object()
                                .and_then(|o| o.get("level"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("books5");
                            pub_args.push(ws::sub_depth(inst_type, sym, level));
                        }
                        "Trade" => {
                            pub_args.push(ws::sub_trade(inst_type, sym));
                        }
                        "Kline" => {
                            let interval = params.as_object()
                                .and_then(|o| o.get("interval"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("1m");
                            pub_args.push(ws::sub_kline(inst_type, sym, interval));
                        }
                        "FundingRate" | "Funding" => {
                            pub_args.push(ws::sub_funding_rate(sym));
                        }
                        "MarkPrice" => {
                            // Bitget 的 ticker 包含 markPrice
                            pub_args.push(ws::sub_ticker(inst_type, sym));
                        }

                        // ===== 私有频道 =====
                        "Order" | "OrderAndFill" => {
                            priv_args.push(ws::sub_orders(inst_type));
                        }
                        "Position" => {
                            priv_args.push(ws::sub_positions(inst_type));
                        }
                        "Balance" => {
                            priv_args.push(ws::sub_account(inst_type));
                        }

                        _ => {}
                    }
                }
            }
        }
    }

    // 去重私有频道
    let mut seen = HashSet::new();
    priv_args.retain(|v| seen.insert(v.to_string()));

    // 构造公共频道结果
    let pub_result = if !pub_args.is_empty() {
        Some((
            config.get_ws_public_url().to_string(),
            vec![json!({"op": "subscribe", "args": pub_args})],
        ))
    } else {
        None
    };

    // 构造私有频道结果 (login + subscribe 两步)
    let priv_result = if !priv_args.is_empty() && config.has_credentials() {
        let ts = (timestamp_ms() / 1000).to_string();
        let sign = signing::sign_ws_login(&config.secret, &ts);

        let login_msg = json!({
            "op": "login",
            "args": [{
                "apiKey": config.api_key,
                "passphrase": config.passphrase,
                "timestamp": ts,
                "sign": sign,
            }]
        });
        let sub_msg = json!({"op": "subscribe", "args": priv_args});

        Some((
            config.get_ws_private_url().to_string(),
            vec![login_msg, sub_msg],
        ))
    } else {
        None
    };

    (pub_result, priv_result)
}

/// 从策略参数中提取 symbol 列表
fn extract_symbols(params: &Value) -> Vec<String> {
    if let Some(arr) = params.as_array() {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    } else if let Some(obj) = params.as_object() {
        obj.get("symbols")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default()
    } else if let Some(s) = params.as_str() {
        vec![s.to_string()]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_subscribe_messages() {
        let config = BitgetConfig::new("test_key", "test_secret", "test_pass", "swap");

        let subs = vec![
            json!({"Bbo": ["BTC_USDT", "ETH_USDT"]}),
            json!({"Depth": {"symbols": ["BTC_USDT"], "level": "books15"}}),
            json!({"Trade": ["BTC_USDT"]}),
            json!({"Order": ["BTC_USDT"]}),
            json!({"Position": ["BTC_USDT"]}),
            json!({"Balance": ["USDT"]}),
        ];

        let (pub_result, priv_result) = build_subscribe_messages(&config, &subs);

        // 公共频道应有: 2个 ticker + 1个 depth + 1个 trade = 4 个 args
        assert!(pub_result.is_some());
        let (url, msgs) = pub_result.unwrap();
        assert!(url.contains("public"));
        assert_eq!(msgs.len(), 1); // 一条 subscribe 消息
        let args = msgs[0].get("args").unwrap().as_array().unwrap();
        assert_eq!(args.len(), 4);

        // 私有频道应有: login + subscribe (3个去重的 args)
        assert!(priv_result.is_some());
        let (url, msgs) = priv_result.unwrap();
        assert!(url.contains("private"));
        assert_eq!(msgs.len(), 2); // login + subscribe
        assert_eq!(msgs[0].get("op").unwrap().as_str().unwrap(), "login");
        assert_eq!(msgs[1].get("op").unwrap().as_str().unwrap(), "subscribe");
    }

    #[test]
    fn test_extract_symbols() {
        // 数组格式
        assert_eq!(extract_symbols(&json!(["BTC_USDT", "ETH_USDT"])), vec!["BTC_USDT", "ETH_USDT"]);
        // 对象格式
        assert_eq!(extract_symbols(&json!({"symbols": ["BTC_USDT"]})), vec!["BTC_USDT"]);
        // 字符串格式
        assert_eq!(extract_symbols(&json!("BTC_USDT")), vec!["BTC_USDT"]);
    }
}
