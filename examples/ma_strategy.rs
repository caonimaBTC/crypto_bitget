//! MA 均线筛选量化策略
//!
//! 复刻 FMZ ianzeng123 的均线筛选策略，使用 D 组最优参数
//!
//! 核心逻辑（每分钟循环）:
//!   1. 获取 Top40 标的（按24h成交额排序）
//!   2. 拉 240 根 H1 K线 → 回测筛选 → 白名单 Top5
//!   3. 用最新 H1 均线计算穿叉信号
//!   4. 信号驱动调仓（金叉做多/死叉做空/信号消失平仓）
//!   5. 止损保护（-5%）
//!
//! 用法: cargo run --example ma_strategy

use crypto_bitget::exchange::bitget::*;
use crypto_bitget::web::{WebServer, WebState};
use crypto_bitget::logger::Logger;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

// ==================== 策略配置 ====================

#[derive(Deserialize)]
struct Config {
    exchange: ExchangeConfig,
    web: Option<WebConfig>,
    log: Option<LogConfig>,
    strategy: Option<StrategyConfig>,
}

#[derive(Deserialize)]
struct ExchangeConfig {
    key: String,
    secret: String,
    passphrase: String,
}

#[derive(Deserialize)]
struct WebConfig {
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
}

#[derive(Deserialize)]
struct LogConfig {
    level: Option<String>,
}

#[derive(Deserialize, Clone)]
struct StrategyConfig {
    top_n: Option<usize>,
    top_coins: Option<usize>,
    kline_limit: Option<u32>,
    ma_params: Option<String>,          // "5_20,10_30,20_60,25_99"
    min_signals: Option<usize>,
    min_win_rate: Option<f64>,
    min_profit_factor: Option<f64>,
    max_mdd: Option<f64>,
    position_ratio: Option<f64>,
    leverage: Option<u32>,
    fixed_capital: Option<f64>,
    allow_short: Option<bool>,
    stop_loss_pct: Option<f64>,
    loop_interval_secs: Option<u64>,
    warmup_rounds: Option<u64>,
}

// ==================== 策略状态 ====================

struct StrategyState {
    // 参数
    top_n: usize,
    top_coins: usize,
    kline_limit: u32,
    ma_params: Vec<(usize, usize)>,     // [(fast, slow), ...]
    min_signals: usize,
    min_win_rate: f64,
    min_profit_factor: f64,
    max_mdd: f64,
    position_ratio: f64,
    leverage: u32,
    fixed_capital: f64,
    allow_short: bool,
    stop_loss_pct: f64,
    loop_interval: u64,
    warmup_rounds: u64,        // 热机轮数，前N轮只观察不交易

    // 运行状态
    run_count: u64,
    init_equity: f64,
    whitelist: Vec<WhitelistItem>,
    positions: HashMap<String, PositionInfo>,   // symbol -> info
    instruments: HashMap<String, Value>,        // symbol -> instrument info
    trade_log: Vec<TradeRecord>,
}

#[derive(Clone)]
struct WhitelistItem {
    symbol: String,
    coin: String,
    score: f64,
    win_rate: f64,
    profit_factor: f64,
    max_dd: f64,
    best_fast: usize,
    best_slow: usize,
}

#[derive(Clone)]
struct PositionInfo {
    symbol: String,
    coin: String,
    side: String,           // "long" / "short"
    entry_price: f64,
    amount: f64,            // USDT 价值
    open_time: u64,
    max_pnl_pct: f64,
    best_fast: usize,
    best_slow: usize,
}

#[derive(Clone, Serialize)]
struct TradeRecord {
    time: String,
    coin: String,
    side: String,
    entry: f64,
    exit_price: f64,
    pnl_pct: f64,
    pnl_usd: f64,
    reason: String,
}

impl StrategyState {
    fn new(cfg: Option<StrategyConfig>) -> Self {
        let c = cfg.unwrap_or(StrategyConfig {
            top_n: None, top_coins: None, kline_limit: None, ma_params: None,
            min_signals: None, min_win_rate: None, min_profit_factor: None,
            max_mdd: None, position_ratio: None, leverage: None, fixed_capital: None,
            allow_short: None, stop_loss_pct: None, loop_interval_secs: None, warmup_rounds: None,
        });

        let ma_str = c.ma_params.unwrap_or_else(|| "5_20,10_30,20_60,25_99".into());
        let ma_params: Vec<(usize, usize)> = ma_str.split(',')
            .filter_map(|p| {
                let parts: Vec<&str> = p.trim().split('_').collect();
                if parts.len() == 2 {
                    Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
                } else { None }
            }).collect();

        StrategyState {
            top_n: c.top_n.unwrap_or(40),
            top_coins: c.top_coins.unwrap_or(5),
            kline_limit: c.kline_limit.unwrap_or(240),
            ma_params,
            min_signals: c.min_signals.unwrap_or(6),
            min_win_rate: c.min_win_rate.unwrap_or(0.40),
            min_profit_factor: c.min_profit_factor.unwrap_or(1.20),
            max_mdd: c.max_mdd.unwrap_or(20.0),
            position_ratio: c.position_ratio.unwrap_or(0.80),
            leverage: c.leverage.unwrap_or(3),
            fixed_capital: c.fixed_capital.unwrap_or(500.0),
            allow_short: c.allow_short.unwrap_or(true),
            stop_loss_pct: c.stop_loss_pct.unwrap_or(5.0),
            loop_interval: c.loop_interval_secs.unwrap_or(60),
            warmup_rounds: c.warmup_rounds.unwrap_or(3),
            run_count: 0,
            init_equity: 0.0,
            whitelist: vec![],
            positions: HashMap::new(),
            instruments: HashMap::new(),
            trade_log: vec![],
        }
    }
}

// ==================== 均线计算 ====================

fn calc_ma(closes: &[f64], period: usize) -> Option<f64> {
    if closes.len() < period { return None; }
    Some(closes[closes.len()-period..].iter().sum::<f64>() / period as f64)
}

fn backtest_ma(closes: &[f64], fast: usize, slow: usize, fee: f64) -> Option<(f64, f64, f64, usize, f64)> {
    // 返回: (win_rate, profit_factor, max_dd, signal_count, total_return)
    if closes.len() < slow + 10 { return None; }
    let n = closes.len();

    let mut trades: Vec<f64> = vec![];
    let mut position: Option<(&str, f64)> = None; // (side, entry)
    let mut equity = 1.0f64;
    let mut peak = 1.0f64;
    let mut max_dd = 0.0f64;

    for i in slow..n {
        let fast_cur: f64 = closes[i+1-fast..=i].iter().sum::<f64>() / fast as f64;
        let fast_prev: f64 = closes[i-fast..i].iter().sum::<f64>() / fast as f64;
        let slow_cur: f64 = closes[i+1-slow..=i].iter().sum::<f64>() / slow as f64;
        let slow_prev: f64 = closes[i-slow..i].iter().sum::<f64>() / slow as f64;

        let cross_up = fast_prev <= slow_prev && fast_cur > slow_cur;
        let cross_down = fast_prev >= slow_prev && fast_cur < slow_cur;

        if let Some((side, entry)) = position {
            let should_close = (side == "long" && cross_down) || (side == "short" && cross_up);
            if should_close {
                let mut ret = (closes[i] - entry) / entry;
                if side == "short" { ret = -ret; }
                ret -= fee * 2.0;
                equity *= 1.0 + ret;
                peak = peak.max(equity);
                max_dd = max_dd.max((peak - equity) / peak * 100.0);
                trades.push(ret);
                position = None;
            }
        }

        if position.is_none() {
            if cross_up {
                position = Some(("long", closes[i]));
            } else if cross_down {
                position = Some(("short", closes[i]));
            }
        }
    }

    // 平掉未结束的持仓
    if let Some((side, entry)) = position {
        let mut ret = (closes[n-1] - entry) / entry;
        if side == "short" { ret = -ret; }
        ret -= fee * 2.0;
        equity *= 1.0 + ret;
        peak = peak.max(equity);
        max_dd = max_dd.max((peak - equity) / peak * 100.0);
        trades.push(ret);
    }

    if trades.is_empty() { return None; }

    let wins: Vec<f64> = trades.iter().filter(|r| **r > 0.0).cloned().collect();
    let losses: Vec<f64> = trades.iter().filter(|r| **r <= 0.0).cloned().collect();
    let win_rate = wins.len() as f64 / trades.len() as f64;
    let avg_win = if wins.is_empty() { 0.0 } else { wins.iter().sum::<f64>() / wins.len() as f64 };
    let avg_loss = if losses.is_empty() { 0.0 } else { (losses.iter().sum::<f64>() / losses.len() as f64).abs() };
    let pf = if avg_loss > 0.0 { avg_win / avg_loss } else if avg_win > 0.0 { 99.0 } else { 0.0 };
    let total_return = (equity - 1.0) * 100.0;

    Some((win_rate, pf, max_dd, trades.len(), total_return))
}

fn save_trade_csv(record: &TradeRecord) {
    use std::io::Write;
    let path = "trades.csv";
    let need_header = !std::path::Path::new(path).exists();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        if need_header {
            let _ = writeln!(f, "time,coin,side,entry,exit,pnl_pct,pnl_usd,reason");
        }
        let _ = writeln!(f, "{},{},{},{:.6},{:.6},{:.2},{:.2},\"{}\"",
            record.time, record.coin, record.side, record.entry, record.exit_price,
            record.pnl_pct, record.pnl_usd, record.reason.replace('"', ""));
    }
}

fn save_equity_csv(equity: f64, upnl: f64) {
    use std::io::Write;
    let path = "equity.csv";
    let need_header = !std::path::Path::new(path).exists();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        if need_header {
            let _ = writeln!(f, "time,equity,unrealized_pnl");
        }
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "{},{:.2},{:.2}", now, equity, upnl);
    }
}

fn calc_score(win_rate: f64, pf: f64, max_dd: f64, max_mdd: f64) -> f64 {
    (win_rate * 100.0).min(100.0) * 0.30
        + (pf * 20.0).min(60.0) * 0.30
        + (1.0 - max_dd / max_mdd).max(0.0) * 100.0 * 0.20
        + 0.5 * 10.0  // vol_pct 简化
}

// ==================== 主程序 ====================

#[tokio::main]
async fn main() {
    // 加载配置
    let config_str = std::fs::read_to_string("config.toml")
        .expect("找不到 config.toml，请复制 config.example.toml 并填入 API 信息");
    let config: Config = toml::from_str(&config_str).expect("config.toml 格式错误");

    // 初始化日志
    let log_level = config.log.as_ref().and_then(|l| l.level.clone()).unwrap_or("INFO".into());
    let log = Logger::new(&log_level, Some("strategy.log"));

    // 启动 Web 面板
    let web_cfg = config.web.unwrap_or(WebConfig {
        host: None, port: None, username: None, password: None,
    });
    let web_user = web_cfg.username.unwrap_or("admin".into());
    let web_pass = web_cfg.password.unwrap_or("123456".into());
    let web_host = web_cfg.host.unwrap_or("0.0.0.0".into());
    let web_port = web_cfg.port.unwrap_or(8888);

    let web = Arc::new(WebState::new(&web_user, &web_pass));
    Logger::bind_web_state(web.clone());
    let wc = web.clone();
    tokio::spawn(async move { WebServer::start(wc, &web_host, web_port).await; });

    log.log(&format!("=== MA 均线筛选策略启动 ==="), "INFO", Some("cyan"));
    log.log(&format!("Web 面板: http://0.0.0.0:{} ({}/***)", web_port, web_user), "INFO", Some("green"));

    // 初始化交易所客户端（主循环用）
    let bitget_config = BitgetConfig::new(
        &config.exchange.key,
        &config.exchange.secret,
        &config.exchange.passphrase,
        "swap",
    );
    let rest = BitgetRestClient::new(bitget_config).expect("REST 客户端初始化失败");

    // 第二个客户端（持仓轮询用）
    let bitget_config2 = BitgetConfig::new(
        &config.exchange.key,
        &config.exchange.secret,
        &config.exchange.passphrase,
        "swap",
    );
    let rest2 = BitgetRestClient::new(bitget_config2).expect("REST 客户端2初始化失败");

    // 初始化策略
    let mut state = StrategyState::new(config.strategy);

    log.log(&format!("MA参数: {:?}", state.ma_params), "INFO", None);
    log.log(&format!("白名单: Top{} | 固定本金: {}U | 杠杆: {}x | 止损: {}% | 热机: {}轮",
        state.top_coins, state.fixed_capital, state.leverage, state.stop_loss_pct, state.warmup_rounds), "INFO", None);

    // 获取初始余额
    let bal = rest.get_usdt_balance().await;
    if let Some(ok) = bal.get("Ok") {
        state.init_equity = ok.get("balance").and_then(|v| v.as_f64()).unwrap_or(0.0);
        log.log(&format!("初始资金: {:.2} USDT", state.init_equity), "INFO", Some("green"));
    }

    // 预加载合约信息
    log.log("加载合约信息...", "INFO", None);
    if let Some(ok) = rest.get_all_instruments().await.get("Ok").and_then(|v| v.as_array()) {
        for inst in ok {
            if let Some(sym) = inst.get("symbol").and_then(|v| v.as_str()) {
                state.instruments.insert(sym.to_string(), inst.clone());
            }
        }
        log.log(&format!("加载 {} 个合约", state.instruments.len()), "INFO", None);
    }

    // 启动时同步真实持仓
    sync_positions_from_exchange(&rest, &log, &mut state).await;

    // 共享数据（持仓轮询线程写，主循环读）
    let shared_upnl = Arc::new(parking_lot::RwLock::new(0.0f64));
    let shared_upnl_w = shared_upnl.clone();

    // 启动持仓+余额轮询任务（每秒更新到Web面板）
    let web_poll = web.clone();
    let init_eq = state.init_equity;
    tokio::spawn(async move {
        loop {
            // 查持仓
            let pos_result = rest2.get_positions().await;
            if let Some(positions) = pos_result.get("Ok").and_then(|v| v.as_array()) {
                let upnl: f64 = positions.iter()
                    .map(|p| p.get("unrealized_pnl").and_then(|v| v.as_f64()).unwrap_or(0.0))
                    .sum();
                *shared_upnl_w.write() = upnl;
                web_poll.update_positions(json!(positions));
            }
            // 余额由主循环每60秒查询，这里不重复查
            // 只负责持仓和未实现盈亏的秒级更新
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });

    // ==================== 主循环 ====================
    loop {
        // 检查 Web 控制面板
        {
            let ctrl = web.controls.read();
            if ctrl.force_stop {
                log.log("收到强制停止信号，退出", "WARN", Some("red"));
                break;
            }
            if ctrl.soft_stop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        }

        state.run_count += 1;

        // 获取当前权益（余额 + 未实现盈亏）
        let equity = get_total_equity(&rest).await;
        if equity <= 0.0 {
            log.log("获取权益失败，跳过", "WARN", Some("yellow"));
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            continue;
        }

        let ret_pct = if state.init_equity > 0.0 {
            (equity - state.init_equity) / state.init_equity * 100.0
        } else { 0.0 };

        log.log(&format!("【第 {} 次运行】权益: {:.2} | 收益率: {:.2}% | 白名单: {} 个 | 持仓: {}",
            state.run_count, equity, ret_pct, state.whitelist.len(), state.positions.len()), "INFO", None);

        // Step 1: 获取 Top40 标的（每分钟）
        let top_symbols = get_top_symbols(&rest, state.top_n).await;
        if top_symbols.is_empty() {
            log.log("标的池为空，跳过", "WARN", Some("yellow"));
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            continue;
        }

        // Step 2: 均线回测筛选（每分钟，跟FMZ一样）
        let whitelist = screen_coins(&rest, &log, &top_symbols, &state).await;
        if !whitelist.is_empty() {
            state.whitelist = whitelist;
        }

        // Step 3: 计算穿叉信号（每分钟）
        let signals = calc_signals(&rest, &log, &state).await;

        // Step 4: 调仓（热机期只观察不交易）
        if state.run_count <= state.warmup_rounds {
            log.log(&format!("[热机] 第 {}/{} 轮，只观察不交易", state.run_count, state.warmup_rounds), "INFO", Some("yellow"));
        } else {
            let ctrl_opening_stopped = web.controls.read().opening_stopped;
            let ctrl_force_closing = web.controls.read().force_closing;

            rebalance(&rest, &log, &mut state, &signals, equity,
                      ctrl_opening_stopped, ctrl_force_closing).await;

            // Step 5: 检查止损
            check_stop_loss(&rest, &log, &mut state).await;
        }

        // 更新 Web 面板
        let upnl = *shared_upnl.read();
        update_web_dashboard(&web, &state, equity, ret_pct, upnl);

        // 每分钟保存权益曲线到CSV
        save_equity_csv(equity, upnl);

        // 等待下一轮
        tokio::time::sleep(std::time::Duration::from_secs(state.loop_interval)).await;
    }
}

// ==================== 启动时同步持仓 ====================

async fn sync_positions_from_exchange(
    rest: &BitgetRestClient,
    log: &Logger,
    state: &mut StrategyState,
) {
    let result = rest.get_positions().await;
    let positions = match result.get("Ok").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return,
    };

    for p in positions {
        let sym = p.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
        let amount = p.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if sym.is_empty() || amount == 0.0 { continue; }

        let side_raw = p.get("side").and_then(|v| v.as_str()).unwrap_or("");
        let side = if side_raw == "Long" || side_raw == "long" { "long" } else { "short" };
        let entry = p.get("entry_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let leverage = p.get("leverage").and_then(|v| v.as_f64()).unwrap_or(state.leverage as f64);
        let coin = sym.replace("_USDT", "");
        // 名义值 = entry * amount，保证金 = 名义值 / 杠杆
        let margin = if leverage > 0.0 { entry * amount / leverage } else { entry * amount };

        state.positions.insert(sym.to_string(), PositionInfo {
            symbol: sym.to_string(),
            coin: coin.clone(),
            side: side.into(),
            entry_price: entry,
            amount: margin,
            open_time: timestamp_ms(),
            max_pnl_pct: 0.0,
            best_fast: 5,
            best_slow: 20,
        });

        log.log(&format!("[同步持仓] {} {} | 入场: {:.6} | 量: {:.4}",
            coin, if side == "long" { "多" } else { "空" }, entry, amount), "INFO", Some("cyan"));
    }

    if state.positions.is_empty() {
        log.log("[同步持仓] 当前无持仓", "INFO", None);
    }
}

// ==================== 获取总权益 ====================

async fn get_total_equity(rest: &BitgetRestClient) -> f64 {
    // accountEquity 已经包含了未实现盈亏，直接用
    let bal = rest.get_usdt_balance().await;
    bal.get("Ok")
        .and_then(|v| v.get("balance"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

// ==================== Step 1: 获取 Top N 标的 ====================

async fn get_top_symbols(rest: &BitgetRestClient, top_n: usize) -> Vec<(String, f64)> {
    let result = rest.get_all_tickers().await;
    let tickers = match result.get("Ok").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return vec![],
    };

    let mut ranked: Vec<(String, f64)> = tickers.iter()
        .filter_map(|t| {
            let sym = t.get("symbol")?.as_str()?;
            if !sym.ends_with("USDT") { return None; }
            let qv = t.get("quote_volume")?.as_f64().unwrap_or(0.0);
            if qv > 0.0 { Some((sym.to_string(), qv)) } else { None }
        })
        .collect();

    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    ranked.truncate(top_n);
    ranked
}

// ==================== Step 2: 回测筛选 ====================

async fn screen_coins(
    rest: &BitgetRestClient,
    log: &Logger,
    top_symbols: &[(String, f64)],
    state: &StrategyState,
) -> Vec<WhitelistItem> {
    log.log(&format!("[均线筛选] 开始 | 标的: {} | K线: {}根H1 | MA: {:?}",
        top_symbols.len(), state.kline_limit, state.ma_params), "INFO", None);

    let mut results: Vec<WhitelistItem> = vec![];

    for (sym, _vol) in top_symbols {
        let coin = sym.replace("_USDT", "");

        // 拉取 H1 K线
        let kline_result = rest.get_klines(sym, "1h", state.kline_limit).await;
        let klines = match kline_result.get("Ok").and_then(|v| v.as_array()) {
            Some(arr) if arr.len() >= 100 => arr,
            _ => continue,
        };

        let closes: Vec<f64> = klines.iter()
            .filter_map(|k| k.get("close")?.as_f64())
            .collect();

        if closes.len() < 100 { continue; }

        // 对每组 MA 参数做回测
        let mut best_score = f64::NEG_INFINITY;
        let mut best: Option<WhitelistItem> = None;

        for &(fast, slow) in &state.ma_params {
            if let Some((wr, pf, mdd, sigs, _ret)) = backtest_ma(&closes, fast, slow, 0.0005) {
                if sigs < state.min_signals || wr < state.min_win_rate
                    || pf < state.min_profit_factor || mdd > state.max_mdd {
                    continue;
                }
                let score = calc_score(wr, pf, mdd, state.max_mdd);
                if score > best_score {
                    best_score = score;
                    best = Some(WhitelistItem {
                        symbol: sym.clone(), coin: coin.clone(), score,
                        win_rate: wr, profit_factor: pf, max_dd: mdd,
                        best_fast: fast, best_slow: slow,
                    });
                }
            }
        }

        if let Some(item) = best {
            results.push(item);
        }

        // 避免请求过快
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    results.truncate(state.top_coins);

    log.log(&format!("[均线筛选] 完成 | 通过: {} | 白名单 Top{}:", results.len(), state.top_coins), "INFO", None);
    for r in &results {
        log.log(&format!("  {} | 评分: {:.1} | 胜率: {:.1}% | 盈亏比: {:.2} | MDD: {:.2}% | MA{}/{}",
            r.coin, r.score, r.win_rate * 100.0, r.profit_factor, r.max_dd, r.best_fast, r.best_slow), "INFO", None);
    }

    results
}

// ==================== Step 3: 计算穿叉信号 ====================

async fn calc_signals(
    rest: &BitgetRestClient,
    log: &Logger,
    state: &StrategyState,
) -> HashMap<String, String> {
    let mut signals: HashMap<String, String> = HashMap::new(); // symbol -> "long"/"short"

    if state.whitelist.is_empty() && state.positions.is_empty() { return signals; }

    log.log(&format!("[计算信号] 白名单: {} 个 | 持仓: {} 个 | 允许做空: {}",
        state.whitelist.len(), state.positions.len(), state.allow_short), "INFO", None);

    // 合并白名单 + 已持仓的币（防止白名单变化导致意外平仓）
    let mut check_list: Vec<WhitelistItem> = state.whitelist.clone();
    for (sym, pos) in &state.positions {
        if !check_list.iter().any(|w| w.symbol == *sym) {
            check_list.push(WhitelistItem {
                symbol: sym.clone(),
                coin: pos.coin.clone(),
                score: 0.0,
                win_rate: 0.0,
                profit_factor: 0.0,
                max_dd: 0.0,
                best_fast: pos.best_fast,
                best_slow: pos.best_slow,
            });
        }
    }

    for item in &check_list {
        let kline_result = rest.get_klines(&item.symbol, "1h", (item.best_slow + 5) as u32).await;
        let klines = match kline_result.get("Ok").and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        let closes: Vec<f64> = klines.iter()
            .filter_map(|k| k.get("close")?.as_f64())
            .collect();

        if closes.len() < item.best_slow + 1 { continue; }

        let fast_cur = calc_ma(&closes, item.best_fast).unwrap_or(0.0);
        let fast_prev = calc_ma(&closes[..closes.len()-1], item.best_fast).unwrap_or(0.0);
        let slow_cur = calc_ma(&closes, item.best_slow).unwrap_or(0.0);
        let slow_prev = calc_ma(&closes[..closes.len()-1], item.best_slow).unwrap_or(0.0);

        if slow_cur == 0.0 { continue; }

        let diff_pct = (fast_cur - slow_cur) / slow_cur * 100.0;
        let cross_up = fast_prev <= slow_prev && fast_cur > slow_cur;
        let cross_down = fast_prev >= slow_prev && fast_cur < slow_cur;

        // 穿叉信号：开新仓
        if cross_up {
            log.log(&format!("  [信号] {} 金叉 MA{}/{} | 差值: {:.4}%",
                item.coin, item.best_fast, item.best_slow, diff_pct), "INFO", Some("green"));
            signals.insert(item.symbol.clone(), "long".into());
        } else if cross_down && state.allow_short {
            log.log(&format!("  [信号] {} 死叉 MA{}/{} | 差值: {:.4}%",
                item.coin, item.best_fast, item.best_slow, diff_pct), "INFO", Some("red"));
            signals.insert(item.symbol.clone(), "short".into());
        } else if fast_cur > slow_cur {
            if let Some(pos) = state.positions.get(&item.symbol) {
                if pos.side == "long" {
                    // 持多仓 + fast仍在slow上方 → 保持做多信号
                    signals.insert(item.symbol.clone(), "long".into());
                    log.log(&format!("  [信号] {} MA{}/{} 多头保持 | 差值: {:.4}%",
                        item.coin, item.best_fast, item.best_slow, diff_pct), "DEBUG", None);
                }
                // 持空仓 + fast在slow上方 → 不插入信号 → 触发"信号消失"平仓
            }
        } else if fast_cur < slow_cur {
            if let Some(pos) = state.positions.get(&item.symbol) {
                if pos.side == "short" && state.allow_short {
                    // 持空仓 + fast仍在slow下方 → 保持做空信号
                    signals.insert(item.symbol.clone(), "short".into());
                    log.log(&format!("  [信号] {} MA{}/{} 空头保持 | 差值: {:.4}%",
                        item.coin, item.best_fast, item.best_slow, diff_pct), "DEBUG", None);
                }
                // 持多仓 + fast在slow下方 → 不插入信号 → 触发"信号消失"平仓
            }
        } else {
            log.log(&format!("  [信号] {} MA{}/{} 无叉 | 差值: {:.4}%",
                item.coin, item.best_fast, item.best_slow, diff_pct), "DEBUG", None);
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let longs = signals.values().filter(|v| *v == "long").count();
    let shorts = signals.values().filter(|v| *v == "short").count();
    log.log(&format!("[计算信号] 汇总 | 做多: {} | 做空: {}", longs, shorts), "INFO", None);

    signals
}

// ==================== Step 4: 调仓 ====================

async fn rebalance(
    rest: &BitgetRestClient,
    log: &Logger,
    state: &mut StrategyState,
    signals: &HashMap<String, String>,
    equity: f64,
    opening_stopped: bool,
    force_closing: bool,
) {
    // 查实际持仓
    let real_positions = rest.get_positions().await;
    let real_pos: Vec<Value> = real_positions.get("Ok")
        .and_then(|v| v.as_array()).cloned().unwrap_or_default();

    // 强制平仓
    if force_closing {
        log.log("[调仓] 收到强制平仓指令", "WARN", Some("red"));
        for p in &real_pos {
            let sym = p.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            let amt = p.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if amt > 0.0 {
                let _ = rest.close_position(sym, None).await;
                log.log(&format!("[平仓] {} 强制平仓", sym), "WARN", Some("red"));
            }
        }
        state.positions.clear();
        return;
    }

    // 需要平仓的：信号消失或方向反转
    let to_close: Vec<String> = state.positions.keys()
        .filter(|sym| {
            match signals.get(*sym) {
                None => true,  // 信号消失
                Some(side) => {
                    let pos_side = &state.positions[*sym].side;
                    side != pos_side  // 方向反转
                }
            }
        })
        .cloned()
        .collect();

    for sym in &to_close {
        let pos = state.positions.get(sym).unwrap().clone();

        // 获取当前价格算盈亏
        let ticker_result = rest.request_raw("GET", "/api/v2/mix/market/ticker", false,
            Some(&json!({"symbol": to_bitget_symbol(sym), "productType": "USDT-FUTURES"})), None).await;
        let exit_price = ticker_result.get("Ok")
            .and_then(|v| v.get("data"))
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|t| t.get("lastPr"))
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(pos.entry_price);

        let result = rest.close_position(&sym, None).await;
        let reason = if signals.contains_key(sym) { "方向反转" } else { "信号消失" };
        let direction = if pos.side == "long" { "多" } else { "空" };

        let mut pnl_pct = (exit_price - pos.entry_price) / pos.entry_price * 100.0;
        if pos.side == "short" { pnl_pct = -pnl_pct; }
        let notional = pos.amount * state.leverage as f64;
        let fee = notional * 0.0006;  // taker手续费
        let pnl_usd = notional * pnl_pct / 100.0 - fee;

        if result.get("Ok").is_some() {
            log.log(&format!("[平仓] {} 平{} | {:.2}% ${:.2} | 手续费: ${:.2} | {}",
                pos.coin, direction, pnl_pct, pnl_usd, fee, reason), "INFO", Some("yellow"));

            let record = TradeRecord {
                time: chrono::Local::now().format("%m-%d %H:%M").to_string(),
                coin: pos.coin.clone(),
                side: direction.into(),
                entry: pos.entry_price,
                exit_price,
                pnl_pct,
                pnl_usd,
                reason: reason.into(),
            };
            save_trade_csv(&record);
            state.trade_log.push(record);
            state.positions.remove(sym);
        } else {
            log.log(&format!("[平仓] {} 失败: {:?}", pos.coin, result.get("Err")), "ERROR", Some("red"));
        }
    }

    // 需要开仓的：有信号但没持仓
    if opening_stopped {
        if !signals.is_empty() {
            log.log("[调仓] 已禁止开仓，跳过新开仓", "WARN", Some("yellow"));
        }
        return;
    }

    // 查交易所真实持仓，防止重复开仓
    let real_syms: Vec<String> = real_pos.iter()
        .filter_map(|p| {
            let amt = p.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if amt > 0.0 { p.get("symbol").and_then(|v| v.as_str()).map(|s| s.to_string()) } else { None }
        })
        .collect();

    let to_open: Vec<(&String, &String)> = signals.iter()
        .filter(|(sym, _)| !state.positions.contains_key(*sym) && !real_syms.contains(sym))
        .collect();

    if to_open.is_empty() { return; }

    // 检查余额是否够开仓
    let available = {
        let bal = rest.get_usdt_balance().await;
        bal.get("Ok").and_then(|v| v.get("available_balance")).and_then(|v| v.as_f64()).unwrap_or(0.0)
    };
    if available < state.fixed_capital * 0.5 {
        log.log(&format!("[调仓] 可用余额 {:.2}U 不足，跳过开仓", available), "WARN", Some("yellow"));
        return;
    }

    // 固定金额模式：500U 保证金 × 3倍杠杆 = 1500U 名义值
    // 按比例模式：权益 × 仓位系数 / 信号数
    let n_total = signals.len().max(1);
    let single_amt = if state.fixed_capital > 0.0 {
        state.fixed_capital
    } else {
        equity * state.position_ratio / n_total as f64
    };

    log.log(&format!("[调仓] 单仓保证金: {:.0}U | 杠杆: {}x | 名义值: {:.0}U | 新开: {}",
        single_amt, state.leverage, single_amt * state.leverage as f64, to_open.len()), "INFO", None);

    for (sym, side) in &to_open {
        // 获取当前价格
        let ticker_result = rest.request_raw("GET", "/api/v2/mix/market/ticker", false,
            Some(&json!({"symbol": to_bitget_symbol(sym), "productType": "USDT-FUTURES"})), None).await;

        let price = ticker_result.get("Ok")
            .and_then(|v| v.get("data"))
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|t| t.get("lastPr"))
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        if price <= 0.0 {
            log.log(&format!("[开仓] {} 获取价格失败", sym), "ERROR", Some("red"));
            continue;
        }

        // 计算下单量
        let inst = state.instruments.get(*sym);
        let min_qty = inst.and_then(|i| i.get("min_qty")).and_then(|v| v.as_f64()).unwrap_or(0.001);
        let step = inst.and_then(|i| i.get("step_size")).and_then(|v| v.as_f64()).unwrap_or(0.001);

        // 名义值 = 保证金 × 杠杆
        let notional = single_amt * state.leverage as f64;
        let qty_raw = notional / price;
        let qty = (qty_raw / step).floor() * step;

        if qty < min_qty {
            log.log(&format!("[开仓] {} 下单量 {:.6} < 最小 {:.6}，跳过", sym, qty, min_qty), "WARN", None);
            continue;
        }

        // 设置杠杆
        let _ = rest.set_leverage(sym, state.leverage, None).await;

        // 下市价单
        let order_side = if *side == "long" { "Buy" } else { "Sell" };
        let coin = sym.replace("_USDT", "");
        let direction = if *side == "long" { "多" } else { "空" };

        let order = PlaceOrderRequest {
            symbol: sym.to_string(),
            side: order_side.into(),
            order_type: "Market".into(),
            amount: qty,
            price: None,
            cid: None,
            pos_side: None,
            time_in_force: "GTC".into(),
            reduce_only: false,
        };

        let result = rest.place_order(&order).await;
        if result.get("Ok").is_some() {
            // 手续费估算: 名义值 × taker费率
            let fee_est = notional * 0.0006;
            log.log(&format!("[开{}] {} | 价: {} | 量: {:.4} | 保证金: ${:.0} | 名义值: ${:.0} | 手续费: ~${:.2}",
                direction, coin, price, qty, single_amt, notional, fee_est), "INFO", Some("green"));

            let coin_clone = coin.clone();

            // 找到白名单中对应的 MA 参数
            let (bf, bs) = state.whitelist.iter()
                .find(|w| &w.symbol == *sym)
                .map(|w| (w.best_fast, w.best_slow))
                .unwrap_or((5, 20));

            state.positions.insert(sym.to_string(), PositionInfo {
                symbol: sym.to_string(),
                coin,
                side: side.to_string(),
                entry_price: price,
                amount: single_amt,
                open_time: timestamp_ms(),
                max_pnl_pct: 0.0,
                best_fast: bf,
                best_slow: bs,
            });

            // 记录开仓到交易日志
            let record = TradeRecord {
                time: chrono::Local::now().format("%m-%d %H:%M").to_string(),
                coin: coin_clone,
                side: direction.into(),
                entry: price,
                exit_price: 0.0,
                pnl_pct: 0.0,
                pnl_usd: -fee_est,
                reason: format!("开仓 ${:.0}x{}", single_amt, state.leverage),
            };
            save_trade_csv(&record);
            state.trade_log.push(record);
        } else {
            log.log(&format!("[开仓] {} 失败: {:?}", sym, result.get("Err")), "ERROR", Some("red"));
        }

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

// ==================== Step 5: 止损检查 ====================

async fn check_stop_loss(
    rest: &BitgetRestClient,
    log: &Logger,
    state: &mut StrategyState,
) {
    if state.stop_loss_pct <= 0.0 { return; }

    let to_check: Vec<String> = state.positions.keys().cloned().collect();

    for sym in to_check {
        let pos = match state.positions.get(&sym) {
            Some(p) => p.clone(),
            None => continue,
        };

        // 获取当前价格
        let ticker_result = rest.request_raw("GET", "/api/v2/mix/market/ticker", false,
            Some(&json!({"symbol": to_bitget_symbol(&sym), "productType": "USDT-FUTURES"})), None).await;

        let price = ticker_result.get("Ok")
            .and_then(|v| v.get("data"))
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|t| t.get("lastPr"))
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        if price <= 0.0 { continue; }

        let mut pnl_pct = (price - pos.entry_price) / pos.entry_price * 100.0;
        if pos.side == "short" { pnl_pct = -pnl_pct; }

        if pnl_pct <= -state.stop_loss_pct {
            let result = rest.close_position(&sym, None).await;
            let direction = if pos.side == "long" { "多" } else { "空" };

            if result.get("Ok").is_some() {
                let sl_notional = pos.amount * state.leverage as f64;
                let sl_fee = sl_notional * 0.0006;
                let sl_pnl = sl_notional * pnl_pct / 100.0 - sl_fee;
                log.log(&format!("[止损] {} 平{} | 亏损: {:.2}% ${:.2} | 手续费: ${:.2} | 入场: {} | 现价: {}",
                    pos.coin, direction, pnl_pct, sl_pnl, sl_fee, pos.entry_price, price), "WARN", Some("red"));

                let record = TradeRecord {
                    time: chrono::Local::now().format("%m-%d %H:%M").to_string(),
                    coin: pos.coin.clone(),
                    side: direction.into(),
                    entry: pos.entry_price,
                    exit_price: price,
                    pnl_pct,
                    pnl_usd: sl_pnl,
                    reason: format!("止损({:.1}%)", pnl_pct),
                };
                save_trade_csv(&record);
                state.trade_log.push(record);
                state.positions.remove(&sym);
            } else {
                log.log(&format!("[止损] {} 平仓失败: {:?}", pos.coin, result.get("Err")), "ERROR", Some("red"));
            }
        }
    }
}

// ==================== 面板更新 ====================

fn update_web_dashboard(web: &Arc<WebState>, state: &StrategyState, equity: f64, ret_pct: f64, unrealized_pnl: f64) {
    let wins = state.trade_log.iter().filter(|t| t.pnl_usd > 0.0).count();
    let total = state.trade_log.len();
    let win_rate = if total > 0 { wins as f64 / total as f64 * 100.0 } else { 0.0 };
    let total_pnl: f64 = state.trade_log.iter().map(|t| t.pnl_usd).sum();

    // 白名单表格 — 行数据用数组格式，列名用 "cols"
    let wl_rows: Vec<Value> = state.whitelist.iter().map(|w| {
        json!([
            w.coin,
            format!("{:.1}", w.score),
            format!("{:.1}%", w.win_rate * 100.0),
            format!("{:.2}", w.profit_factor),
            format!("{:.2}%", w.max_dd),
            format!("{}/{}", w.best_fast, w.best_slow),
        ])
    }).collect();

    // 交易记录表格
    let trade_rows: Vec<Value> = state.trade_log.iter().rev().take(50).map(|t| {
        json!([
            t.time,
            t.coin,
            t.side,
            format!("{:.6}", t.entry),
            format!("{:.6}", t.exit_price),
            format!("{:+.2}%", t.pnl_pct),
            format!("${:+.2}", t.pnl_usd),
            t.reason,
        ])
    }).collect();

    // 持仓数据 — 推送标准格式给前端
    let pos_data: Vec<Value> = state.positions.values().map(|p| {
        json!({
            "symbol": p.symbol,
            "side": if p.side == "long" { "Long" } else { "Short" },
            "amount": p.amount / p.entry_price,  // 近似持仓量
            "entry_price": p.entry_price,
            "mark_price": p.entry_price,  // 没有实时标记价，用入场价
            "unrealized_pnl": 0.0,
            "leverage": state.leverage,
            "margin_mode": "crossed",
        })
    }).collect();

    // 计算盈亏明细
    let total_profit: f64 = state.trade_log.iter().filter(|t| t.pnl_usd > 0.0).map(|t| t.pnl_usd).sum();
    let total_loss: f64 = state.trade_log.iter().filter(|t| t.pnl_usd <= 0.0).map(|t| t.pnl_usd).sum();

    // 每个白名单币种的策略数据
    let strategies: Vec<Value> = state.whitelist.iter().map(|w| {
        let pos = state.positions.get(&w.symbol);
        let coin_trades: Vec<&TradeRecord> = state.trade_log.iter().filter(|t| t.coin == w.coin).collect();
        let coin_wins = coin_trades.iter().filter(|t| t.pnl_usd > 0.0).count();
        let coin_pnl: f64 = coin_trades.iter().map(|t| t.pnl_usd).sum();
        let coin_wr = if coin_trades.is_empty() { 0.0 } else { coin_wins as f64 / coin_trades.len() as f64 };

        json!({
            "name": w.coin,
            "symbol": w.symbol,
            "exchange": "Bitget",
            "order_type": format!("MA{}/{}", w.best_fast, w.best_slow),
            "position_side": pos.map(|p| if p.side == "long" { "Long" } else { "Short" }).unwrap_or(""),
            "position_amount": pos.map(|p| p.amount * state.leverage as f64 / p.entry_price).unwrap_or(0.0),
            "balance": equity,
            "count": coin_trades.len(),
            "win_rate": coin_wr,
            "profit": coin_trades.iter().filter(|t| t.pnl_usd > 0.0).map(|t| t.pnl_usd).sum::<f64>(),
            "loss": coin_trades.iter().filter(|t| t.pnl_usd <= 0.0).map(|t| t.pnl_usd).sum::<f64>(),
            "total_profit": coin_pnl,
        })
    }).collect();

    // unrealized_pnl 从持仓轮询线程获取（参数传入）

    // 计算当日盈亏
    let today_str = chrono::Local::now().format("%m-%d").to_string();
    let today_trades: Vec<&TradeRecord> = state.trade_log.iter()
        .filter(|t| t.time.starts_with(&today_str)).collect();
    let today_profit: f64 = today_trades.iter().map(|t| t.pnl_usd).sum();
    let today_wins = today_trades.iter().filter(|t| t.pnl_usd > 0.0).count();
    let today_wr = if today_trades.is_empty() { 0.0 } else { today_wins as f64 / today_trades.len() as f64 };

    web.update_stats(json!({
        "current_balance": equity,
        "initial_balance": state.init_equity,
        "available_balance": equity,
        "total_profit": total_pnl,
        "total_loss": total_loss,
        "unrealized_pnl": unrealized_pnl,
        "today_profit": today_profit,
        "frozen_balance": 0,
        "funding_fee": 0,
        "volume": 0,
        "win_rate": if total > 0 { wins as f64 / total as f64 } else { 0.0 },
        "count": total,
        "success_count": wins,
        "failure_count": total - wins,
        "return_pct": ret_pct,
        "server_name": "MA均线策略",
        "strategies": strategies,
        "today": {
            "count": today_trades.len(),
            "success_count": today_wins,
            "failure_count": today_trades.len() - today_wins,
            "win_rate": today_wr,
            "profit": today_trades.iter().filter(|t| t.pnl_usd > 0.0).map(|t| t.pnl_usd).sum::<f64>(),
            "loss": today_trades.iter().filter(|t| t.pnl_usd <= 0.0).map(|t| t.pnl_usd).sum::<f64>(),
            "total_profit": today_profit,
        },
    }));

    // 持仓由轮询任务独立更新，这里不再重复推
    web.update_tables(vec![
        json!({"title": "白名单", "cols": ["币种","评分","胜率","盈亏比","MDD","MA"], "rows": wl_rows}),
        json!({"title": "交易记录", "cols": ["时间","币种","方向","入场","出场","盈亏%","盈亏$","原因"], "rows": trade_rows}),
    ]);
}
