use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ==================== Bitget v2 API 常量 ====================

/// REST API 基础 URL
pub const REST_BASE_URL: &str = "https://api.bitget.com";

/// WebSocket 公共频道 URL
pub const WS_PUBLIC_URL: &str = "wss://ws.bitget.com/v2/ws/public";

/// WebSocket 私有频道 URL
pub const WS_PRIVATE_URL: &str = "wss://ws.bitget.com/v2/ws/private";

// ==================== REST API 路径 ====================

pub mod path {
    // 合约
    pub const MIX_ACCOUNT: &str = "/api/v2/mix/account/accounts";
    pub const MIX_POSITIONS: &str = "/api/v2/mix/position/all-position";
    pub const MIX_PLACE_ORDER: &str = "/api/v2/mix/order/place-order";
    pub const MIX_CANCEL_ORDER: &str = "/api/v2/mix/order/cancel-order";
    pub const MIX_OPEN_ORDERS: &str = "/api/v2/mix/order/orders-pending";
    pub const MIX_ORDER_DETAIL: &str = "/api/v2/mix/order/detail";
    pub const MIX_BATCH_ORDERS: &str = "/api/v2/mix/order/batch-place-order";
    pub const MIX_BATCH_CANCEL: &str = "/api/v2/mix/order/batch-cancel-orders";
    pub const MIX_CLOSE_POSITIONS: &str = "/api/v2/mix/order/close-positions";
    pub const MIX_SET_LEVERAGE: &str = "/api/v2/mix/account/set-leverage";
    pub const MIX_SET_MARGIN_MODE: &str = "/api/v2/mix/account/set-margin-mode";
    pub const MIX_SET_POSITION_MODE: &str = "/api/v2/mix/account/set-position-mode";
    pub const MIX_TICKERS: &str = "/api/v2/mix/market/tickers";
    pub const MIX_TICKER: &str = "/api/v2/mix/market/ticker";
    pub const MIX_DEPTH: &str = "/api/v2/mix/market/merge-depth";
    pub const MIX_KLINES: &str = "/api/v2/mix/market/candles";
    pub const MIX_TRADES: &str = "/api/v2/mix/market/fills";
    pub const MIX_INSTRUMENTS: &str = "/api/v2/mix/market/contracts";
    pub const MIX_FUNDING_RATE: &str = "/api/v2/mix/market/current-fund-rate";

    // 现货
    pub const SPOT_ACCOUNT: &str = "/api/v2/spot/account/assets";
    pub const SPOT_PLACE_ORDER: &str = "/api/v2/spot/trade/place-order";
    pub const SPOT_CANCEL_ORDER: &str = "/api/v2/spot/trade/cancel-order";
    pub const SPOT_OPEN_ORDERS: &str = "/api/v2/spot/trade/unfilled-orders";
    pub const SPOT_ORDER_DETAIL: &str = "/api/v2/spot/trade/orderInfo";
    pub const SPOT_BATCH_ORDERS: &str = "/api/v2/spot/trade/batch-orders";
    pub const SPOT_BATCH_CANCEL: &str = "/api/v2/spot/trade/batch-cancel-order";
    pub const SPOT_TICKERS: &str = "/api/v2/spot/market/tickers";
    pub const SPOT_TICKER: &str = "/api/v2/spot/market/ticker";
    pub const SPOT_DEPTH: &str = "/api/v2/spot/market/merge-depth";
    pub const SPOT_KLINES: &str = "/api/v2/spot/market/candles";
    pub const SPOT_TRADES: &str = "/api/v2/spot/market/fills";
    pub const SPOT_INSTRUMENTS: &str = "/api/v2/spot/public/symbols";
}

// ==================== 产品类型 ====================

pub const PRODUCT_USDT_FUTURES: &str = "USDT-FUTURES";
pub const PRODUCT_COIN_FUTURES: &str = "COIN-FUTURES";
pub const PRODUCT_USDC_FUTURES: &str = "USDC-FUTURES";

// ==================== 配置 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitgetConfig {
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
    #[serde(default)]
    pub market_type: String,      // "swap" | "spot"
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub rest_url: Option<String>,  // 自定义 REST URL
    #[serde(default)]
    pub ws_public_url: Option<String>,
    #[serde(default)]
    pub ws_private_url: Option<String>,
}

impl BitgetConfig {
    pub fn new(api_key: &str, secret: &str, passphrase: &str, market_type: &str) -> Self {
        BitgetConfig {
            api_key: api_key.to_string(),
            secret: secret.to_string(),
            passphrase: passphrase.to_string(),
            market_type: market_type.to_string(),
            proxy: None,
            rest_url: None,
            ws_public_url: None,
            ws_private_url: None,
        }
    }

    pub fn has_credentials(&self) -> bool {
        !self.api_key.is_empty() && !self.secret.is_empty()
    }

    pub fn get_rest_url(&self) -> &str {
        self.rest_url.as_deref().unwrap_or(REST_BASE_URL)
    }

    pub fn get_ws_public_url(&self) -> &str {
        self.ws_public_url.as_deref().unwrap_or(WS_PUBLIC_URL)
    }

    pub fn get_ws_private_url(&self) -> &str {
        self.ws_private_url.as_deref().unwrap_or(WS_PRIVATE_URL)
    }

    pub fn is_swap(&self) -> bool {
        self.market_type == "swap"
    }

    pub fn product_type(&self) -> &str {
        if self.is_swap() { PRODUCT_USDT_FUTURES } else { "SPOT" }
    }

    pub fn inst_type(&self) -> &str {
        if self.is_swap() { "USDT-FUTURES" } else { "SPOT" }
    }
}

// ==================== 统一数据结构 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub balance: f64,
    pub available_balance: f64,
    pub frozen_balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: String,         // "Long" | "Short"
    pub amount: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub leverage: f64,
    pub margin_mode: String,  // "crossed" | "isolated"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub cid: String,
    pub symbol: String,
    pub side: String,         // "Buy" | "Sell"
    pub order_type: String,   // "Limit" | "Market"
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub avg_price: f64,
    pub status: String,       // "Open" | "PartiallyFilled" | "Filled" | "Canceled"
    pub trade_side: String,   // "open" | "close"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub bid_price: f64,
    pub bid_size: f64,
    pub ask_price: f64,
    pub ask_size: f64,
    pub last_price: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    pub symbol: String,
    pub base: String,
    pub quote: String,
    pub min_qty: f64,
    pub tick_size: f64,
    pub step_size: f64,
    pub min_notional: f64,
    pub contract_size: f64,
    pub is_trading: bool,
    pub market_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub symbol: String,
    pub interval: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub symbol: String,
    pub price: f64,
    pub amount: f64,
    pub side: String,         // "Buy" | "Sell"
    pub timestamp: u64,
}

// ==================== 下单参数 ====================

#[derive(Debug, Clone)]
pub struct PlaceOrderRequest {
    pub symbol: String,
    pub side: String,          // "Buy" | "Sell"
    pub order_type: String,    // "Limit" | "Market"
    pub amount: f64,
    pub price: Option<f64>,
    pub cid: Option<String>,
    pub pos_side: Option<String>,  // "Long" | "Short"
    pub time_in_force: String,     // "GTC" | "IOC" | "FOK" | "PostOnly"
    pub reduce_only: bool,
}

impl Default for PlaceOrderRequest {
    fn default() -> Self {
        PlaceOrderRequest {
            symbol: String::new(),
            side: "Buy".into(),
            order_type: "Limit".into(),
            amount: 0.0,
            price: None,
            cid: None,
            pos_side: None,
            time_in_force: "GTC".into(),
            reduce_only: false,
        }
    }
}

// ==================== 工具函数 ====================

/// 转换内部符号为 Bitget v2 格式: BTC_USDT -> BTCUSDT
pub fn to_bitget_symbol(symbol: &str) -> String {
    symbol.replace("_", "")
}

/// 转换 Bitget 格式为内部符号: BTCUSDT -> BTC_USDT (兼容 v1 _UMCBL/_SPBL)
pub fn from_bitget_symbol(s: &str) -> String {
    // 移除 v1 遗留后缀
    let s = s.replace("_UMCBL", "").replace("_SPBL", "");
    if s.contains('_') { return s; }
    // 按优先级尝试匹配 quote 币种
    for suffix in &["USDT", "USDC", "BUSD", "TUSD", "DAI"] {
        if s.ends_with(suffix) && s.len() > suffix.len() {
            return format!("{}_{}", &s[..s.len() - suffix.len()], suffix);
        }
    }
    s
}

/// 解析 JSON 数值字段 (支持字符串或数字)
pub fn pf(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|val| val.as_f64().or_else(|| val.as_str().and_then(|s| s.parse().ok())))
        .unwrap_or(0.0)
}

/// 解析 JSON 字符串字段
pub fn ps<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|val| val.as_str()).unwrap_or("")
}

/// 成功返回值
pub fn ok_result(data: Value) -> Value {
    json!({"Ok": data})
}

/// 错误返回值
pub fn err_result(msg: &str) -> Value {
    json!({"Err": msg})
}

/// 当前毫秒时间戳
pub fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_millis() as u64
}
