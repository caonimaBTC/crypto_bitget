use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::signing;
use super::types::*;

/// Bitget v2 REST API 客户端
///
/// 仅负责账户查询和交易操作，行情数据通过 WebSocket 获取
pub struct BitgetRestClient {
    pub config: BitgetConfig,
    client: Client,
}

impl BitgetRestClient {
    pub fn new(config: BitgetConfig) -> Result<Self> {
        let mut builder = Client::builder();
        if let Some(ref proxy_url) = config.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy_url)?);
        }
        let client = builder
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(BitgetRestClient { config, client })
    }

    // ==================== 底层签名请求 ====================

    /// 发送签名请求
    async fn request(&self, method: &str, path: &str, params: &HashMap<String, String>, body: Option<&Value>, auth: bool) -> Result<Value> {
        let base_url = self.config.get_rest_url();

        // 排序参数确保签名一致性
        let mut sorted_params: Vec<_> = params.iter().collect();
        sorted_params.sort_by_key(|(k, _)| (*k).clone());
        let qs: String = sorted_params.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        // GET/DELETE 手动拼接 URL，POST body 不走 query string
        let full_url = if !qs.is_empty() && method != "POST" {
            format!("{}{}?{}", base_url, path, qs)
        } else {
            format!("{}{}", base_url, path)
        };

        // 签名用的 body 字符串必须和发送的一致
        let body_str = match body {
            Some(b) => serde_json::to_string(b)
                .map_err(|e| anyhow::anyhow!("Body 序列化失败: {}", e))?,
            None => String::new(),
        };

        let mut req = match method {
            "POST" => self.client.post(&full_url).header("Content-Type", "application/json"),
            "DELETE" => self.client.delete(&full_url).header("Content-Type", "application/json"),
            _ => self.client.get(&full_url),
        };

        if auth {
            let ts = timestamp_ms().to_string();
            let sign = signing::sign_request(&self.config.secret, &ts, method, path, &qs, &body_str);

            req = req
                .header("ACCESS-KEY", &self.config.api_key)
                .header("ACCESS-SIGN", &sign)
                .header("ACCESS-TIMESTAMP", &ts)
                .header("ACCESS-PASSPHRASE", &self.config.passphrase)
                .header("locale", "en-US");
        }

        if body.is_some() {
            // 用与签名一致的字符串作为 body，避免 .json() 序列化差异
            req = req.body(body_str.clone());
        }

        let resp = req.send().await?;
        let text = resp.text().await?;
        Ok(serde_json::from_str(&text).unwrap_or(json!(text)))
    }

    /// 公共请求 (无签名)
    async fn public_request(&self, method: &str, path: &str, params: &HashMap<String, String>) -> Result<Value> {
        self.request(method, path, params, None, false).await
    }

    /// 签名请求
    async fn signed_request(&self, method: &str, path: &str, params: &HashMap<String, String>, body: Option<&Value>) -> Result<Value> {
        self.request(method, path, params, body, true).await
    }

    /// 通用原始请求 (供策略调用任意端点)
    pub async fn request_raw(&self, method: &str, path: &str, auth: bool, query: Option<&Value>, body: Option<&Value>) -> Value {
        let mut params = HashMap::new();
        if let Some(q) = query {
            if let Some(obj) = q.as_object() {
                for (k, v) in obj {
                    params.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
                }
            }
        }
        match self.request(method, path, &params, body, auth).await {
            Ok(d) => ok_result(d),
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 账户 API ====================

    /// 获取 USDT 余额
    pub async fn get_usdt_balance(&self) -> Value {
        let path = if self.config.is_swap() { path::MIX_ACCOUNT } else { path::SPOT_ACCOUNT };
        let mut params = HashMap::new();
        if self.config.is_swap() {
            params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());
        }

        match self.signed_request("GET", path, &params, None).await {
            Ok(data) => self.parse_balance(&data),
            Err(e) => err_result(&e.to_string()),
        }
    }

    fn parse_balance(&self, data: &Value) -> Value {
        if let Some(d) = data.get("data") {
            if self.config.is_swap() {
                // 合约: data 是数组 [{marginCoin, accountEquity, available, ...}]
                if let Some(arr) = d.as_array() {
                    for item in arr {
                        if ps(item, "marginCoin") == "USDT" || arr.len() == 1 {
                            return ok_result(json!({
                                "balance": pf(item, "accountEquity"),
                                "available_balance": pf(item, "available"),
                                "frozen_balance": pf(item, "locked"),
                            }));
                        }
                    }
                }
                // 兼容: 也许某些接口返回对象
                if d.is_object() {
                    return ok_result(json!({
                        "balance": pf(d, "accountEquity"),
                        "available_balance": pf(d, "available"),
                        "frozen_balance": pf(d, "locked"),
                    }));
                }
            } else {
                // 现货: data 是数组
                if let Some(arr) = d.as_array() {
                    for item in arr {
                        if ps(item, "coin") == "USDT" {
                            return ok_result(json!({
                                "balance": pf(item, "available") + pf(item, "frozen"),
                                "available_balance": pf(item, "available"),
                                "frozen_balance": pf(item, "frozen"),
                            }));
                        }
                    }
                }
            }
        }
        ok_result(json!({"balance": 0, "available_balance": 0, "frozen_balance": 0}))
    }

    // ==================== 持仓 API ====================

    /// 获取所有持仓
    pub async fn get_positions(&self) -> Value {
        if !self.config.is_swap() {
            return ok_result(json!([]));
        }

        let mut params = HashMap::new();
        params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());

        match self.signed_request("GET", path::MIX_POSITIONS, &params, None).await {
            Ok(data) => self.parse_positions(&data),
            Err(e) => err_result(&e.to_string()),
        }
    }

    fn parse_positions(&self, data: &Value) -> Value {
        let mut positions = vec![];
        if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
            for p in arr {
                let size = pf(p, "total");
                if size.abs() > 0.0 {
                    let hold_side = ps(p, "holdSide").to_lowercase();
                    positions.push(json!({
                        "symbol": from_bitget_symbol(ps(p, "symbol")),
                        "side": match hold_side.as_str() {
                            "long" => "Long", "short" => "Short", _ => ps(p, "holdSide")
                        },
                        "amount": size.abs(),
                        "entry_price": pf(p, "openPriceAvg"),
                        "mark_price": pf(p, "markPrice"),
                        "unrealized_pnl": pf(p, "unrealizedPL"),
                        "liquidation_price": pf(p, "liquidationPrice"),
                        "leverage": pf(p, "leverage"),
                        "margin_mode": ps(p, "marginMode"),
                    }));
                }
            }
        }
        ok_result(json!(positions))
    }

    // ==================== 下单 API ====================

    /// 下单
    pub async fn place_order(&self, req: &PlaceOrderRequest) -> Value {
        // 合约 reduce_only (平仓) → 直接用闪电平仓接口，兼容对冲模式
        if self.config.is_swap() && req.reduce_only {
            let hold_side = match req.side.as_str() {
                "Sell" => "long",   // 卖平多
                "Buy" => "short",   // 买平空
                _ => "long",
            };
            return self.close_position(&req.symbol, Some(hold_side)).await;
        }

        let path = if self.config.is_swap() { path::MIX_PLACE_ORDER } else { path::SPOT_PLACE_ORDER };
        let bg_sym = to_bitget_symbol(&req.symbol);

        let is_market = req.order_type == "Market";
        let force = match req.time_in_force.as_str() {
            "PostOnly" => "post_only", "IOC" => "ioc", "FOK" => "fok", _ => "gtc",
        };

        let body = if self.config.is_swap() {
            // 合约开仓
            let trade_side = match req.pos_side.as_deref() {
                Some("Long") | Some("Short") => "open",
                Some(s) => s,
                None => "open",
            };
            let mut body = json!({
                "symbol": bg_sym,
                "productType": PRODUCT_USDT_FUTURES,
                "marginMode": "crossed",
                "marginCoin": "USDT",
                "side": req.side.to_lowercase(),
                "orderType": if is_market { "market" } else { "limit" },
                "size": req.amount.to_string(),
                "tradeSide": trade_side,
                "force": force,
            });
            if let Some(ref cid) = req.cid {
                if !cid.is_empty() { body["clientOid"] = json!(cid); }
            }
            // 限价单才带 price
            if !is_market {
                if let Some(px) = req.price {
                    body["price"] = json!(px.to_string());
                }
            }
            body
        } else {
            // 现货下单
            let mut body = json!({
                "symbol": bg_sym,
                "side": req.side.to_lowercase(),
                "orderType": if is_market { "market" } else { "limit" },
                "size": req.amount.to_string(),
                "force": force,
            });
            if let Some(ref cid) = req.cid {
                if !cid.is_empty() { body["clientOid"] = json!(cid); }
            }
            if !is_market {
                if let Some(px) = req.price {
                    body["price"] = json!(px.to_string());
                }
            }
            body
        };

        match self.signed_request("POST", path, &HashMap::new(), Some(&body)).await {
            Ok(data) => self.parse_order_response(&data),
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 通用下单 (兼容统一接口, order 为 JSON)
    pub async fn place_order_json(&self, order: &Value, _params: &Value) -> Value {
        let req = PlaceOrderRequest {
            symbol: ps(order, "symbol").to_string(),
            side: ps(order, "side").to_string(),
            order_type: ps(order, "order_type").to_string(),
            amount: pf(order, "amount"),
            price: order.get("price").and_then(|v| v.as_f64()),
            cid: order.get("cid").and_then(|v| v.as_str()).map(|s| s.to_string()),
            pos_side: order.get("pos_side").and_then(|v| v.as_str()).map(|s| s.to_string()),
            time_in_force: order.get("time_in_force").and_then(|v| v.as_str()).unwrap_or("GTC").to_string(),
            reduce_only: order.get("reduce_only").and_then(|v| v.as_bool()).unwrap_or(false),
        };
        self.place_order(&req).await
    }

    fn parse_order_response(&self, data: &Value) -> Value {
        // 先检查错误码
        if let Some(code) = data.get("code").and_then(|v| v.as_str()) {
            if code != "00000" {
                let msg = ps(data, "msg");
                return err_result(&format!("Bitget error {}: {}", code, msg));
            }
        }
        // 提取订单 ID
        if let Some(d) = data.get("data") {
            let oid = d.get("orderId")
                .or_else(|| d.get("ordId"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !oid.is_empty() {
                return ok_result(json!(oid));
            }
        }
        err_result(&data.to_string())
    }

    // ==================== 闪电平仓 ====================

    /// 闪电平仓 (对冲模式和单向模式通用)
    /// hold_side: "long" | "short" (对冲模式必填)
    pub async fn close_position(&self, symbol: &str, hold_side: Option<&str>) -> Value {
        if !self.config.is_swap() {
            return err_result("闪电平仓仅支持合约");
        }

        let mut body = json!({
            "symbol": to_bitget_symbol(symbol),
            "productType": PRODUCT_USDT_FUTURES,
            "marginCoin": "USDT",
        });
        if let Some(side) = hold_side {
            body["holdSide"] = json!(side);
        }

        match self.signed_request("POST", path::MIX_CLOSE_POSITIONS, &HashMap::new(), Some(&body)).await {
            Ok(data) => {
                if let Some(code) = data.get("code").and_then(|v| v.as_str()) {
                    if code != "00000" {
                        return err_result(&format!("Bitget close error {}: {}", code, ps(&data, "msg")));
                    }
                }
                ok_result(data)
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 全部平仓 (平掉指定币对的所有持仓)
    pub async fn close_all_positions(&self, symbol: &str) -> Value {
        if !self.config.is_swap() {
            return err_result("仅合约支持");
        }
        // 先查持仓
        let positions = self.get_positions().await;
        if let Some(ok) = positions.get("Ok") {
            if let Some(arr) = ok.as_array() {
                let target = symbol.replace("_", "");
                for p in arr {
                    let sym = ps(p, "symbol").replace("_", "");
                    if sym.is_empty() || sym == target || symbol == "*" {
                        let hold_side = match ps(p, "side") {
                            "Long" => "long",
                            "Short" => "short",
                            other => {
                                eprintln!("[Bitget] close_all: 未知持仓方向 '{}', 跳过", other);
                                continue;
                            }
                        };
                        let result = self.close_position(
                            &ps(p, "symbol").to_string(),
                            Some(hold_side),
                        ).await;
                        if result.get("Err").is_some() {
                            return result;
                        }
                    }
                }
                return ok_result(json!("all closed"));
            }
        }
        err_result("查持仓失败")
    }

    // ==================== 批量下单 ====================

    /// 批量下单 (最多50个)
    /// 批量下单 (最多50个)
    /// 注意: reduce_only 的订单不能用批量接口, 请单独调用 close_position()
    pub async fn batch_place_orders(&self, orders: &[PlaceOrderRequest]) -> Value {
        // 检查: 批量下单不支持 reduce_only (对冲模式下会失败)
        if orders.iter().any(|o| o.reduce_only) {
            return err_result("批量下单不支持 reduce_only, 请用 close_position() 平仓");
        }

        let path = if self.config.is_swap() { path::MIX_BATCH_ORDERS } else { path::SPOT_BATCH_ORDERS };

        let order_list: Vec<Value> = orders.iter().map(|req| {
            let bg_sym = to_bitget_symbol(&req.symbol);
            let is_market = req.order_type == "Market";
            let force = match req.time_in_force.as_str() {
                "PostOnly" => "post_only", "IOC" => "ioc", "FOK" => "fok", _ => "gtc",
            };
            if self.config.is_swap() {
                let trade_side = match req.pos_side.as_deref() {
                    Some("Long") | Some("Short") => "open",
                    _ => "open",
                };
                let mut item = json!({
                    "symbol": bg_sym,
                    "productType": PRODUCT_USDT_FUTURES,
                    "marginMode": "crossed",
                    "marginCoin": "USDT",
                    "side": req.side.to_lowercase(),
                    "orderType": if is_market { "market" } else { "limit" },
                    "size": req.amount.to_string(),
                    "tradeSide": trade_side,
                    "force": force,
                });
                if let Some(ref cid) = req.cid { if !cid.is_empty() { item["clientOid"] = json!(cid); } }
                if !is_market { if let Some(px) = req.price { item["price"] = json!(px.to_string()); } }
                item
            } else {
                let mut item = json!({
                    "symbol": bg_sym,
                    "side": req.side.to_lowercase(),
                    "orderType": if is_market { "market" } else { "limit" },
                    "size": req.amount.to_string(),
                    "force": force,
                });
                if let Some(ref cid) = req.cid { if !cid.is_empty() { item["clientOid"] = json!(cid); } }
                if !is_market { if let Some(px) = req.price { item["price"] = json!(px.to_string()); } }
                item
            }
        }).collect();

        let body = json!({ "orderList": order_list });
        match self.signed_request("POST", path, &HashMap::new(), Some(&body)).await {
            Ok(data) => {
                // 检查错误码
                if let Some(code) = data.get("code").and_then(|v| v.as_str()) {
                    if code != "00000" {
                        return err_result(&format!("Bitget batch error {}: {}", code, ps(&data, "msg")));
                    }
                }
                ok_result(data)
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 撤单 API ====================

    /// 撤单 (orderId 和 clientOid 至少提供一个)
    pub async fn cancel_order(&self, symbol: &str, order_id: Option<&str>, cid: Option<&str>) -> Value {
        if order_id.unwrap_or("").is_empty() && cid.unwrap_or("").is_empty() {
            return err_result("cancel_order 需要 orderId 或 clientOid 至少一个");
        }

        let path = if self.config.is_swap() { path::MIX_CANCEL_ORDER } else { path::SPOT_CANCEL_ORDER };

        let mut body = json!({
            "symbol": to_bitget_symbol(symbol),
        });
        if let Some(oid) = order_id { if !oid.is_empty() { body["orderId"] = json!(oid); } }
        if let Some(c) = cid { if !c.is_empty() { body["clientOid"] = json!(c); } }

        if self.config.is_swap() {
            body["productType"] = json!(PRODUCT_USDT_FUTURES);
        }

        match self.signed_request("POST", path, &HashMap::new(), Some(&body)).await {
            Ok(data) => ok_result(data),
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 批量撤单
    pub async fn batch_cancel_orders(&self, symbol: &str, order_ids: &[String]) -> Value {
        let path = if self.config.is_swap() { path::MIX_BATCH_CANCEL } else { path::SPOT_BATCH_CANCEL };

        let body = if self.config.is_swap() {
            json!({
                "symbol": to_bitget_symbol(symbol),
                "productType": PRODUCT_USDT_FUTURES,
                "orderIdList": order_ids.iter().map(|id| json!({"orderId": id})).collect::<Vec<_>>(),
            })
        } else {
            json!({
                "symbol": to_bitget_symbol(symbol),
                "orderIdList": order_ids.iter().map(|id| json!({"orderId": id})).collect::<Vec<_>>(),
            })
        };

        match self.signed_request("POST", path, &HashMap::new(), Some(&body)).await {
            Ok(data) => {
                if let Some(code) = data.get("code").and_then(|v| v.as_str()) {
                    if code != "00000" {
                        return err_result(&format!("Bitget batch cancel error {}: {}", code, ps(&data, "msg")));
                    }
                }
                ok_result(data)
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 查询订单 API ====================

    /// 获取挂单
    pub async fn get_open_orders(&self, symbol: &str) -> Value {
        let path = if self.config.is_swap() { path::MIX_OPEN_ORDERS } else { path::SPOT_OPEN_ORDERS };
        let mut params = HashMap::new();
        params.insert("symbol".into(), to_bitget_symbol(symbol));
        if self.config.is_swap() {
            params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());
        }

        match self.signed_request("GET", path, &params, None).await {
            Ok(data) => {
                if let Some(arr) = data.get("data").and_then(|d| d.get("entrustedList").or(Some(d))).and_then(|v| v.as_array()) {
                    let orders: Vec<Value> = arr.iter().map(|o| {
                        json!({
                            "id": ps(o, "orderId"),
                            "cid": ps(o, "clientOid"),
                            "symbol": from_bitget_symbol(ps(o, "symbol")),
                            "side": if ps(o, "side") == "buy" { "Buy" } else { "Sell" },
                            "price": pf(o, "price"),
                            "amount": pf(o, "size"),
                            "filled": pf(o, "baseVolume"),
                            "status": "Open",
                        })
                    }).collect();
                    ok_result(json!(orders))
                } else {
                    ok_result(json!([]))
                }
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 查询单个订单详情
    pub async fn get_order_detail(&self, symbol: &str, order_id: &str) -> Value {
        let path = if self.config.is_swap() { path::MIX_ORDER_DETAIL } else { path::SPOT_ORDER_DETAIL };
        let mut params = HashMap::new();
        params.insert("symbol".into(), to_bitget_symbol(symbol));
        params.insert("orderId".into(), order_id.into());
        if self.config.is_swap() {
            params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());
        }

        match self.signed_request("GET", path, &params, None).await {
            Ok(data) => ok_result(data),
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 合约设置 API ====================

    /// 设置杠杆
    /// hold_side: None = 单向模式, Some("long") / Some("short") = 对冲模式
    pub async fn set_leverage(&self, symbol: &str, leverage: u32, hold_side: Option<&str>) -> Value {
        if !self.config.is_swap() {
            return ok_result(json!(null));
        }

        let mut body = json!({
            "symbol": to_bitget_symbol(symbol),
            "productType": PRODUCT_USDT_FUTURES,
            "marginCoin": "USDT",
            "leverage": leverage.to_string(),
        });

        // 对冲模式需要指定 holdSide
        if let Some(side) = hold_side {
            body["holdSide"] = json!(side);
        }

        self.check_and_return(
            self.signed_request("POST", path::MIX_SET_LEVERAGE, &HashMap::new(), Some(&body)).await
        )
    }

    /// 设置保证金模式 (crossed / isolated)
    pub async fn set_margin_mode(&self, symbol: &str, mode: &str) -> Value {
        if !self.config.is_swap() {
            return ok_result(json!(null));
        }

        let body = json!({
            "symbol": to_bitget_symbol(symbol),
            "productType": PRODUCT_USDT_FUTURES,
            "marginCoin": "USDT",
            "marginMode": mode,  // "crossed" | "isolated"
        });

        self.check_and_return(
            self.signed_request("POST", path::MIX_SET_MARGIN_MODE, &HashMap::new(), Some(&body)).await
        )
    }

    /// 设置持仓模式 (one_way_mode / hedge_mode)
    pub async fn set_position_mode(&self, symbol: &str, mode: &str) -> Value {
        if !self.config.is_swap() {
            return ok_result(json!(null));
        }

        let body = json!({
            "symbol": to_bitget_symbol(symbol),
            "productType": PRODUCT_USDT_FUTURES,
            "holdMode": mode,  // "single_hold" | "double_hold"
        });

        self.check_and_return(
            self.signed_request("POST", path::MIX_SET_POSITION_MODE, &HashMap::new(), Some(&body)).await
        )
    }

    /// 检查 Bitget 响应错误码，统一处理
    fn check_and_return(&self, result: Result<Value>) -> Value {
        match result {
            Ok(data) => {
                if let Some(code) = data.get("code").and_then(|v| v.as_str()) {
                    if code != "00000" {
                        return err_result(&format!("Bitget error {}: {}", code, ps(&data, "msg")));
                    }
                }
                ok_result(data)
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 合约信息 API ====================

    /// 获取合约/币对信息
    pub async fn get_instrument(&self, symbol: &str) -> Value {
        let path = if self.config.is_swap() { path::MIX_INSTRUMENTS } else { path::SPOT_INSTRUMENTS };
        let mut params = HashMap::new();
        if self.config.is_swap() {
            params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());
        }

        match self.public_request("GET", path, &params).await {
            Ok(data) => self.parse_instrument(&data, symbol),
            Err(e) => err_result(&e.to_string()),
        }
    }

    fn parse_instrument(&self, data: &Value, symbol: &str) -> Value {
        let target = to_bitget_symbol(symbol);
        if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
            for inst in arr {
                let inst_sym = ps(inst, "symbol");
                if inst_sym == target {
                    if self.config.is_swap() {
                        return ok_result(json!({
                            "symbol": symbol,
                            "base": ps(inst, "baseCoin"),
                            "quote": ps(inst, "quoteCoin"),
                            "min_qty": pf(inst, "minTradeNum"),
                            "tick_size": 10f64.powi(-(pf(inst, "pricePlace") as i32)),
                            "step_size": 10f64.powi(-(pf(inst, "volumePlace") as i32)),
                            "min_notional": pf(inst, "minTradeUSDT"),
                            "contract_size": pf(inst, "sizeMultiplier"),
                            "is_trading": ps(inst, "symbolStatus") == "normal",
                            "market_type": "swap",
                        }));
                    } else {
                        // Bitget v2 spot: pricePrecision/quantityPrecision 是小数位数
                        let price_prec = pf(inst, "pricePrecision");
                        let qty_prec = pf(inst, "quantityPrecision");
                        return ok_result(json!({
                            "symbol": symbol,
                            "base": ps(inst, "baseCoin"),
                            "quote": ps(inst, "quoteCoin"),
                            "min_qty": pf(inst, "minTradeAmount"),
                            "tick_size": if price_prec > 0.0 { 10f64.powi(-(price_prec as i32)) } else { 0.01 },
                            "step_size": if qty_prec > 0.0 { 10f64.powi(-(qty_prec as i32)) } else { 0.01 },
                            "min_notional": pf(inst, "minTradeUSDT"),
                            "contract_size": 1.0,
                            "is_trading": ps(inst, "status") == "online",
                            "market_type": "spot",
                        }));
                    }
                }
            }
        }
        err_result(&format!("未找到合约: {}", symbol))
    }

    // ==================== 资金费率 ====================

    /// 获取当前资金费率
    pub async fn get_funding_rate(&self, symbol: &str) -> Value {
        if !self.config.is_swap() {
            return err_result("资金费率仅合约支持");
        }

        let mut params = HashMap::new();
        params.insert("symbol".into(), to_bitget_symbol(symbol));
        params.insert("productType".into(), PRODUCT_USDT_FUTURES.into());

        match self.public_request("GET", path::MIX_FUNDING_RATE, &params).await {
            Ok(data) => {
                if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                    if let Some(item) = arr.first() {
                        return ok_result(json!({
                            "symbol": symbol,
                            "funding_rate": pf(item, "fundingRate"),
                        }));
                    }
                }
                err_result("无数据")
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    // ==================== 手续费 ====================

    pub async fn get_fee_rate(&self, symbol: &str) -> Value {
        // Bitget v2 未提供独立的费率查询接口, 返回默认值
        ok_result(json!({"symbol": symbol, "maker": 0.0002, "taker": 0.0006}))
    }

    // ==================== 批量市场数据 API ====================

    /// 获取所有合约 ticker（成交量、最新价等）
    ///
    /// 返回: {"Ok": [{"symbol": "BTC_USDT", "last_price": f64, "quote_volume": f64, ...}, ...]}
    pub async fn get_all_tickers(&self) -> Value {
        let mut params = HashMap::new();
        params.insert("productType".into(), self.config.product_type().into());

        match self.public_request("GET", path::MIX_TICKERS, &params).await {
            Ok(data) => {
                if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                    let tickers: Vec<Value> = arr.iter().map(|t| {
                        json!({
                            "symbol": from_bitget_symbol(ps(t, "symbol")),
                            "last_price": pf(t, "lastPr"),
                            "bid_price": pf(t, "bidPr"),
                            "ask_price": pf(t, "askPr"),
                            "high_24h": pf(t, "high24h"),
                            "low_24h": pf(t, "low24h"),
                            "volume": pf(t, "baseVolume"),
                            "quote_volume": pf(t, "quoteVolume"),
                            "change_percent": pf(t, "change24h"),
                            "timestamp": pf(t, "ts") as u64,
                        })
                    }).collect();
                    ok_result(json!(tickers))
                } else {
                    err_result("tickers 数据格式异常")
                }
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 获取K线数据
    ///
    /// - symbol: "BTC_USDT" 格式
    /// - interval: "1m", "5m", "15m", "30m", "1h", "4h", "1d"
    /// - limit: 最多返回根数 (Bitget 上限 1000)
    ///
    /// 返回: {"Ok": [{"timestamp": u64, "open": f64, "high": f64, "low": f64, "close": f64, "volume": f64}, ...]}
    pub async fn get_klines(&self, symbol: &str, interval: &str, limit: u32) -> Value {
        let granularity = match interval {
            "1m" => "1m", "5m" => "5m", "15m" => "15m", "30m" => "30m",
            "1h" | "1H" => "1H", "4h" | "4H" => "4H",
            "1d" | "1D" => "1D", "1w" | "1W" => "1W",
            other => other,
        };

        let mut params = HashMap::new();
        params.insert("symbol".into(), to_bitget_symbol(symbol));
        params.insert("productType".into(), self.config.product_type().into());
        params.insert("granularity".into(), granularity.into());
        params.insert("limit".into(), limit.to_string());

        match self.public_request("GET", path::MIX_KLINES, &params).await {
            Ok(data) => {
                if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                    let mut klines: Vec<Value> = arr.iter().filter_map(|k| {
                        let items = k.as_array()?;
                        if items.len() < 6 { return None; }
                        Some(json!({
                            "timestamp": items[0].as_str().unwrap_or("0").parse::<u64>().unwrap_or(0),
                            "open": items[1].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "high": items[2].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "low": items[3].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "close": items[4].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                            "volume": items[5].as_str().unwrap_or("0").parse::<f64>().unwrap_or(0.0),
                        }))
                    }).collect();
                    // Bitget 返回降序（最新在前），统一排成升序（最旧在前）
                    klines.sort_by_key(|k| k.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0));
                    ok_result(json!(klines))
                } else {
                    err_result("klines 数据格式异常")
                }
            }
            Err(e) => err_result(&e.to_string()),
        }
    }

    /// 获取所有合约信息
    ///
    /// 返回: {"Ok": [{"symbol": "BTC_USDT", "min_qty": f64, "tick_size": f64, ...}, ...]}
    pub async fn get_all_instruments(&self) -> Value {
        let mut params = HashMap::new();
        params.insert("productType".into(), self.config.product_type().into());

        let path = if self.config.is_swap() { path::MIX_INSTRUMENTS } else { "/api/v2/spot/public/symbols" };

        match self.public_request("GET", path, &params).await {
            Ok(data) => {
                if let Some(arr) = data.get("data").and_then(|d| d.as_array()) {
                    let instruments: Vec<Value> = arr.iter().map(|item| {
                        if self.config.is_swap() {
                            let price_precision = pf(item, "pricePlace") as u32;
                            let qty_precision = pf(item, "volumePlace") as u32;
                            let tick_size = if price_precision > 0 { 1.0 / 10f64.powi(price_precision as i32) } else { 0.01 };
                            let step_size = if qty_precision > 0 { 1.0 / 10f64.powi(qty_precision as i32) } else { 0.01 };
                            json!({
                                "symbol": from_bitget_symbol(ps(item, "symbol")),
                                "base": ps(item, "baseCoin"),
                                "quote": ps(item, "quoteCoin"),
                                "min_qty": pf(item, "minTradeNum"),
                                "tick_size": tick_size,
                                "step_size": step_size,
                                "min_notional": pf(item, "minTradeUSDT"),
                                "contract_size": pf(item, "sizeMultiplier"),
                                "is_trading": ps(item, "symbolStatus") == "normal",
                                "market_type": "swap",
                            })
                        } else {
                            let price_precision = pf(item, "pricePrecision") as u32;
                            let qty_precision = pf(item, "quantityPrecision") as u32;
                            let tick_size = if price_precision > 0 { 1.0 / 10f64.powi(price_precision as i32) } else { 0.01 };
                            let step_size = if qty_precision > 0 { 1.0 / 10f64.powi(qty_precision as i32) } else { 0.01 };
                            json!({
                                "symbol": from_bitget_symbol(ps(item, "symbol")),
                                "base": ps(item, "baseCoin"),
                                "quote": ps(item, "quoteCoin"),
                                "min_qty": pf(item, "minTradeAmount"),
                                "tick_size": tick_size,
                                "step_size": step_size,
                                "min_notional": pf(item, "minTradeUSDT"),
                                "contract_size": 1.0,
                                "is_trading": ps(item, "status") == "online",
                                "market_type": "spot",
                            })
                        }
                    }).collect();
                    ok_result(json!(instruments))
                } else {
                    err_result("instruments 数据格式异常")
                }
            }
            Err(e) => err_result(&e.to_string()),
        }
    }
}
