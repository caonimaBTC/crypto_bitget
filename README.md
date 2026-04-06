# Crypto Bitget 交易框架

Bitget v2 交易所 Rust 框架，支持合约/现货交易 + Web 监控面板。

## 项目结构

```
crypto_bitget/
├── Cargo.toml                    # 项目配置
├── config.example.toml           # 配置示例
├── README.md                     # 本文档
├── src/
│   ├── lib.rs                    # 库入口
│   ├── logger.rs                 # 日志 (控制台 + 文件 + Web推送)
│   ├── exchange/
│   │   ├── mod.rs
│   │   └── bitget/
│   │       ├── mod.rs            # 模块导出
│   │       ├── types.rs          # 数据类型、常量、API路径
│   │       ├── signing.rs        # HMAC-SHA256 Base64 签名
│   │       ├── rest.rs           # REST API 客户端
│   │       ├── ws.rs             # WebSocket 客户端
│   │       └── subscription.rs   # 订阅构造器
│   └── web/
│       ├── mod.rs
│       ├── server.rs             # Axum Web 服务器
│       └── html.rs               # 暗色主题 Dashboard
└── examples/
    ├── basic.rs                  # 基础: 查余额/持仓/价格
    ├── trade.rs                  # 交易: 挂单/撤单/开仓/平仓
    └── with_web.rs               # Web面板: 监控 + 交易
```

## 快速开始

### 1. 安装依赖

需要 Rust 工具链:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows 还需要 MSYS2 + MinGW:
```bash
# 安装后将 C:\msys64\mingw64\bin 加入 PATH
```

### 2. 配置 API

复制配置示例并填入你的 Bitget API Key:
```bash
cp config.example.toml config.toml
```

### 3. 编译运行

```bash
# 运行基础示例
cargo run --example basic --release

# 运行交易示例
cargo run --example trade --release

# 运行 Web 面板示例
cargo run --example with_web --release
```

## API 使用

### 创建客户端

```rust
use crypto_bitget::exchange::bitget::*;

let config = BitgetConfig::new(
    "api_key",
    "secret",
    "passphrase",
    "swap",      // "swap" 合约 | "spot" 现货
);

let rest = BitgetRestClient::new(config).unwrap();
```

### 查询余额

```rust
let balance = rest.get_usdt_balance().await;
// {"Ok": {"balance": 8504.74, "available_balance": 8504.74, "frozen_balance": 0.0}}
```

### 查询持仓

```rust
let positions = rest.get_positions().await;
// {"Ok": [{"symbol":"BTC_USDT","side":"Long","amount":0.001,"entry_price":67000,...}]}
```

### 下单

```rust
// 限价买单
let order = PlaceOrderRequest {
    symbol: "BTC_USDT".into(),
    side: "Buy".into(),           // "Buy" | "Sell"
    order_type: "Limit".into(),   // "Limit" | "Market"
    amount: 0.001,
    price: Some(60000.0),
    cid: Some("my_order_001".into()),
    time_in_force: "GTC".into(),  // "GTC" | "IOC" | "FOK" | "PostOnly"
    ..Default::default()
};
let result = rest.place_order(&order).await;
// {"Ok": "1424589035862601729"}

// 市价买单
let market_order = PlaceOrderRequest {
    symbol: "BTC_USDT".into(),
    side: "Buy".into(),
    order_type: "Market".into(),
    amount: 0.001,
    ..Default::default()
};
let result = rest.place_order(&market_order).await;
```

### 撤单

```rust
// 通过订单ID撤单
let result = rest.cancel_order("BTC_USDT", Some("order_id"), None).await;

// 通过客户端ID撤单
let result = rest.cancel_order("BTC_USDT", None, Some("my_order_001")).await;
```

### 平仓

```rust
// 闪电平仓 (推荐, 兼容对冲模式)
let result = rest.close_position("BTC_USDT", Some("long")).await;   // 平多
let result = rest.close_position("BTC_USDT", Some("short")).await;  // 平空

// 全部平仓
let result = rest.close_all_positions("BTC_USDT").await;

// 也可以用 reduce_only (自动调用闪电平仓)
let close = PlaceOrderRequest {
    symbol: "BTC_USDT".into(),
    side: "Sell".into(),    // Sell 平多, Buy 平空
    order_type: "Market".into(),
    amount: 0.001,
    reduce_only: true,      // 自动走闪电平仓
    ..Default::default()
};
let result = rest.place_order(&close).await;
```

### 查询挂单

```rust
let orders = rest.get_open_orders("BTC_USDT").await;
```

### 设置杠杆

```rust
// 单向模式
rest.set_leverage("BTC_USDT", 20, None).await;

// 对冲模式 (需指定方向)
rest.set_leverage("BTC_USDT", 20, Some("long")).await;
rest.set_leverage("BTC_USDT", 10, Some("short")).await;
```

### 合约信息

```rust
let info = rest.get_instrument("BTC_USDT").await;
// {"Ok": {"symbol":"BTC_USDT","tick_size":0.1,"step_size":0.001,...}}
```

### 通用原始请求

```rust
// 调用任意 Bitget v2 API
let result = rest.request_raw(
    "GET",                    // 方法
    "/api/v2/mix/market/ticker",  // 路径
    false,                    // 是否签名
    Some(&json!({"productType":"USDT-FUTURES","symbol":"BTCUSDT"})),  // 查询参数
    None,                     // 请求体
).await;
```

## WebSocket 行情

```rust
use crypto_bitget::exchange::bitget::{ws, BitgetWsClient, BitgetWsEvent, BitgetConfig};
use tokio::sync::mpsc;

let config = BitgetConfig::new("key", "secret", "pass", "swap");
let (tx, mut rx) = mpsc::unbounded_channel();

// 公共频道 (行情)
let ws_client = BitgetWsClient::new(config.clone());
let pub_args = vec![
    ws::sub_ticker("USDT-FUTURES", "BTC_USDT"),
    ws::sub_depth("USDT-FUTURES", "BTC_USDT", "books5"),
    ws::sub_trade("USDT-FUTURES", "BTC_USDT"),
    ws::sub_kline("USDT-FUTURES", "BTC_USDT", "1m"),
];
tokio::spawn(async move {
    ws_client.connect_public(pub_args, tx).await.unwrap();
});

// 私有频道 (订单/持仓/余额)
let ws_priv = BitgetWsClient::new(config);
let priv_args = vec![
    ws::sub_orders("USDT-FUTURES"),
    ws::sub_positions("USDT-FUTURES"),
    ws::sub_account("USDT-FUTURES"),
];
let priv_tx = rx_clone; // 另一个 sender
tokio::spawn(async move {
    ws_priv.connect_private(priv_args, priv_tx).await.unwrap();
});

// 接收事件
while let Some(event) = rx.recv().await {
    match event {
        BitgetWsEvent::Ticker { symbol, data } => println!("行情: {} {:?}", symbol, data),
        BitgetWsEvent::Order { data } => println!("订单: {:?}", data),
        BitgetWsEvent::Position { data } => println!("持仓: {:?}", data),
        _ => {}
    }
}
```

## Web 监控面板

```rust
use crypto_bitget::web::{WebServer, WebState};
use crypto_bitget::logger::Logger;
use std::sync::Arc;

// 启动面板
let web = Arc::new(WebState::new("admin", "your_password"));
Logger::bind_web_state(web.clone());

tokio::spawn(async move {
    WebServer::start(web, "0.0.0.0", 8888).await;
});

// 打开 http://localhost:8888 登录

// 推送数据到面板
web.update_stats(json!({...}));         // 更新统计
web.update_positions(json!([...]));     // 更新持仓
web.update_tables(vec![json!({...})]);  // 更新表格
web.push_log("消息", "INFO", "green");  // 推送日志

// Logger 的所有日志会自动推送到面板
let log = Logger::new("INFO", None);
log.log("这条日志会显示在面板上", "INFO", Some("green"));
```

### 面板功能

- **主页**: 策略列表表格, 点击进入详情
- **详情页**: 账户信息 / 持仓表 / 风控统计 / 盈利曲线 / 实时日志
- **控制按钮**: 暂停交易 / 停止开仓 / 强制平仓 / 账户强停
- **日志**: 搜索 / 按级别过滤 / 导出
- **认证**: SHA256 密码哈希 + Token (7天有效)

## 支持的 API

| 功能 | 方法 | 说明 |
|------|------|------|
| 查余额 | `get_usdt_balance()` | USDT 余额 |
| 查持仓 | `get_positions()` | 所有持仓 |
| 下单 | `place_order()` | 限价/市价, 开仓/平仓 |
| 撤单 | `cancel_order()` | 按订单ID或客户端ID |
| 平仓 | `close_position()` | 闪电平仓 |
| 全部平仓 | `close_all_positions()` | 平掉所有持仓 |
| 批量下单 | `batch_place_orders()` | 最多50个 |
| 批量撤单 | `batch_cancel_orders()` | 批量撤 |
| 查挂单 | `get_open_orders()` | 当前挂单 |
| 查订单详情 | `get_order_detail()` | 单个订单 |
| 合约信息 | `get_instrument()` | tick_size等 |
| 设置杠杆 | `set_leverage()` | 支持对冲模式 |
| 设置保证金模式 | `set_margin_mode()` | crossed/isolated |
| 设置持仓模式 | `set_position_mode()` | single/hedge |
| 资金费率 | `get_funding_rate()` | 当前费率 |
| 手续费率 | `get_fee_rate()` | maker/taker |
| 通用请求 | `request_raw()` | 任意 API |

## 实盘验证

已通过 Bitget 实盘测试:

| 功能 | 状态 |
|------|------|
| 余额查询 | ✅ 8504.74 USDT |
| 限价挂单 | ✅ |
| 查询挂单 | ✅ |
| 撤单 | ✅ |
| 市价开仓 | ✅ (对冲模式 10x) |
| 查询持仓 | ✅ 含开仓价/标记价/盈亏/杠杆 |
| 实时持仓监控 | ✅ |
| 闪电平仓 | ✅ |
| Web 面板 | ✅ |
