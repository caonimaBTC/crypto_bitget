use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Sha256, Digest};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::broadcast;

use super::html;

// ==================== 数据结构 ====================

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    #[serde(rename = "type")]
    pub typ: String,
    pub timestamp: f64,
    pub time: String,
    pub level: String,
    pub color: String,
    pub msg: String,
}

/// 共享状态 — 所有线程/协程可安全访问
pub struct WebState {
    // 认证
    username: String,
    password_hash: RwLock<String>,
    tokens: RwLock<HashMap<String, f64>>,   // token -> 过期时间
    token_ttl: f64,

    // 日志缓冲
    log_buffer: RwLock<VecDeque<LogEntry>>,
    log_tx: broadcast::Sender<String>,      // 广播给所有 WS 客户端

    // 业务数据 (策略/持仓/控制)
    pub stats: RwLock<Value>,
    pub positions: RwLock<Value>,
    pub tables: RwLock<Vec<Value>>,
    pub controls: RwLock<ControlState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlState {
    pub force_stop: bool,
    pub soft_stop: bool,
    pub opening_stopped: bool,
    pub force_closing: bool,
}

impl Default for ControlState {
    fn default() -> Self {
        ControlState {
            force_stop: false,
            soft_stop: false,
            opening_stopped: false,
            force_closing: false,
        }
    }
}

impl WebState {
    pub fn new(username: &str, password: &str) -> Self {
        let (log_tx, _) = broadcast::channel(1024);
        WebState {
            username: username.to_string(),
            password_hash: RwLock::new(sha256_hex(password)),
            tokens: RwLock::new(HashMap::new()),
            token_ttl: 86400.0 * 7.0, // 7 天
            log_buffer: RwLock::new(VecDeque::with_capacity(500)),
            log_tx,
            stats: RwLock::new(json!({})),
            positions: RwLock::new(json!([])),
            tables: RwLock::new(vec![]),
            controls: RwLock::new(ControlState::default()),
        }
    }

    /// 推送日志 (从 Logger 调用)
    pub fn push_log(&self, msg: &str, level: &str, color: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let time_str = chrono::Local::now().format("%H:%M:%S").to_string();

        let entry = LogEntry {
            typ: "log".into(),
            timestamp: now,
            time: time_str,
            level: level.to_uppercase().trim().to_string(),
            color: color.to_string(),
            msg: msg.to_string(),
        };

        // 缓冲
        {
            let mut buf = self.log_buffer.write();
            if buf.len() >= 500 { buf.pop_front(); }
            buf.push_back(entry.clone());
        }

        // 广播
        let json_str = serde_json::to_string(&entry).unwrap_or_default();
        let _ = self.log_tx.send(json_str);
    }

    /// 更新统计数据
    pub fn update_stats(&self, data: Value) {
        *self.stats.write() = data;
    }

    /// 更新持仓数据
    pub fn update_positions(&self, data: Value) {
        *self.positions.write() = data;
    }

    /// 更新数据表
    pub fn update_tables(&self, data: Vec<Value>) {
        *self.tables.write() = data;
    }

    // 认证
    fn create_token(&self) -> String {
        let token = format!("{:x}", Sha256::digest(
            format!("{}{}", rand::random::<u64>(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ).as_bytes()
        ));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap()
            .as_secs_f64();
        let expire = now + self.token_ttl;
        let mut tokens = self.tokens.write();
        tokens.insert(token.clone(), expire);
        // 顺便清理过期 token
        tokens.retain(|_, &mut exp| exp > now);
        token
    }

    fn verify_token(&self, token: &str) -> bool {
        if token.is_empty() { return false; }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap()
            .as_secs_f64();
        let mut tokens = self.tokens.write();
        if let Some(&expire) = tokens.get(token) {
            if expire > now {
                return true;
            }
            // 过期则移除
            tokens.remove(token);
        }
        false
    }
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

// ==================== Web Server ====================

pub struct WebServer;

impl WebServer {
    /// 启动 web 服务 (在 tokio 中 spawn)
    pub async fn start(state: Arc<WebState>, host: &str, port: u16) {
        let app = Router::new()
            // 页面路由
            .route("/login", get(login_page).post(login_submit))
            .route("/logout", get(logout))
            .route("/", get(dashboard_page))
            .route("/strategy/:name", get(strategy_detail_page))
            // API 路由
            .route("/api/stats", get(api_stats))
            .route("/api/logs", get(api_logs))
            .route("/api/tables", get(api_tables))
            .route("/api/controls", get(api_controls))
            .route("/api/control/:action", post(api_control))
            .route("/api/change_password", post(api_change_password))
            // WebSocket
            .route("/ws", get(ws_handler))
            .with_state(state);

        let addr = format!("{}:{}", host, port);
        let listener = tokio::net::TcpListener::bind(&addr).await
            .expect(&format!("无法绑定 {}", addr));

        axum::serve(listener, app).await.unwrap();
    }
}

// ==================== 页面路由 ====================

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn get_token(headers: &axum::http::HeaderMap, query: &TokenQuery) -> String {
    // Cookie
    if let Some(cookie) = headers.get(header::COOKIE) {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("crypto_token=") {
                    return val.to_string();
                }
            }
        }
    }
    // Query param
    if let Some(ref t) = query.token {
        return t.clone();
    }
    // Authorization header
    if let Some(auth) = headers.get(header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            return s.replace("Bearer ", "").to_string();
        }
    }
    String::new()
}

async fn login_page() -> Html<String> {
    Html(html::login_html(None))
}

#[derive(Deserialize)]
struct LoginForm {
    username: Option<String>,
    password: Option<String>,
}

async fn login_submit(
    State(state): State<Arc<WebState>>,
    axum::extract::Form(form): axum::extract::Form<LoginForm>,
) -> Response {
    let username = form.username.unwrap_or_default();
    let password = form.password.unwrap_or_default();

    if username == state.username && sha256_hex(&password) == *state.password_hash.read() {
        let token = state.create_token();
        // 不加 Secure flag, 兼容 localhost HTTP 开发环境
        // 生产环境通过反向代理 (nginx) 强制 HTTPS + 添加 Secure
        let cookie = format!(
            "crypto_token={}; Max-Age={}; Path=/; HttpOnly; SameSite=Lax",
            token, state.token_ttl as u64
        );
        (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, "/".to_string()),
                (header::SET_COOKIE, cookie),
            ],
        ).into_response()
    } else {
        Html(html::login_html(Some("用户名或密码错误"))).into_response()
    }
}

async fn logout(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !token.is_empty() {
        state.tokens.write().remove(&token);
    }
    let cookie = "crypto_token=; Max-Age=0; Path=/; HttpOnly";
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/login".to_string()),
            (header::SET_COOKIE, cookie.to_string()),
        ],
    ).into_response()
}

async fn dashboard_page(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return Redirect::to("/login").into_response();
    }
    Html(html::dashboard_html()).into_response()
}

async fn strategy_detail_page(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return Redirect::to("/login").into_response();
    }
    Html(html::detail_html(&name)).into_response()
}

// ==================== API 路由 ====================

async fn api_stats(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    Json(state.stats.read().clone()).into_response()
}

async fn api_logs(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    let buf = state.log_buffer.read();
    let logs: Vec<&LogEntry> = buf.iter().collect();
    Json(json!(logs)).into_response()
}

async fn api_tables(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    Json(json!(*state.tables.read())).into_response()
}

async fn api_controls(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    Json(json!(*state.controls.read())).into_response()
}

async fn api_control(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    axum::extract::Path(action): axum::extract::Path<String>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    let mut ctrl = state.controls.write();
    match action.as_str() {
        "force_stop" => ctrl.force_stop = !ctrl.force_stop,
        "soft_stop" => ctrl.soft_stop = !ctrl.soft_stop,
        "opening_stopped" => ctrl.opening_stopped = !ctrl.opening_stopped,
        "force_closing" => ctrl.force_closing = !ctrl.force_closing,
        _ => {}
    }
    Json(json!({"ok": true})).into_response()
}

#[derive(Deserialize)]
struct ChangePwdReq {
    old_password: String,
    new_password: String,
}

async fn api_change_password(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    Json(body): Json<ChangePwdReq>,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, Json(json!({"error":"unauthorized"}))).into_response();
    }
    if sha256_hex(&body.old_password) != *state.password_hash.read() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"原密码错误"}))).into_response();
    }
    if body.new_password.len() < 6 {
        return (StatusCode::BAD_REQUEST, Json(json!({"error":"新密码至少6位"}))).into_response();
    }
    *state.password_hash.write() = sha256_hex(&body.new_password);
    // 清除所有 token，强制重新登录
    state.tokens.write().clear();
    Json(json!({"ok": true})).into_response()
}

// ==================== WebSocket ====================

async fn ws_handler(
    State(state): State<Arc<WebState>>,
    headers: axum::http::HeaderMap,
    Query(q): Query<TokenQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let token = get_token(&headers, &q);
    if !state.verify_token(&token) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<WebState>) {
    // 先订阅 broadcast，再读历史，确保不丢失中间日志
    let mut log_rx = state.log_tx.subscribe();

    // 发送历史日志 (clone 出来释放锁, 再异步发送)
    let history: Vec<String> = {
        let buf = state.log_buffer.read();
        buf.iter().map(|e| serde_json::to_string(e).unwrap_or_default()).collect()
    };
    for msg in history {
        if socket.send(Message::Text(msg)).await.is_err() { return; }
    }
    // 历史发送期间 broadcast 到的消息可能和历史有重叠，客户端按 timestamp 去重即可

    // 定时推送 stats/controls/positions
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(2));
    ticker.tick().await; // 跳过首次

    loop {
        tokio::select! {
            // 实时日志
            msg = log_rx.recv() => {
                match msg {
                    Ok(text) => {
                        if socket.send(Message::Text(text)).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // 日志流太快跟不上，跳过丢失的消息继续
                        let warn = serde_json::json!({"type":"log","time":"","level":"WARN","color":"yellow","msg":format!("日志流过快，跳过 {} 条", n),"timestamp":0});
                        let _ = socket.send(Message::Text(warn.to_string())).await;
                    }
                    Err(_) => break,
                }
            }
            // 定时推送业务数据
            _ = ticker.tick() => {
                let stats = state.stats.read().clone();
                let positions = state.positions.read().clone();
                let controls = state.controls.read().clone();
                let tables = state.tables.read().clone();

                let stats_msg = json!({"type": "stats", "data": stats});
                if socket.send(Message::Text(serde_json::to_string(&stats_msg).unwrap_or_default())).await.is_err() { break; }

                let ctrl_msg = json!({"type": "controls", "data": controls});
                if socket.send(Message::Text(serde_json::to_string(&ctrl_msg).unwrap_or_default())).await.is_err() { break; }

                if !tables.is_empty() {
                    let tbl_msg = json!({"type": "tables", "data": tables});
                    if socket.send(Message::Text(serde_json::to_string(&tbl_msg).unwrap_or_default())).await.is_err() { break; }
                }

                let pos_msg = json!({"type": "positions", "data": positions});
                if socket.send(Message::Text(serde_json::to_string(&pos_msg).unwrap_or_default())).await.is_err() { break; }
            }
            // 接收客户端消息 (控制指令)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<Value>(&text) {
                            if cmd.get("action").and_then(|v| v.as_str()) == Some("control") {
                                if let Some(ctrl_name) = cmd.get("control").and_then(|v| v.as_str()) {
                                    let mut ctrl = state.controls.write();
                                    match ctrl_name {
                                        "force_stop" => ctrl.force_stop = !ctrl.force_stop,
                                        "soft_stop" => ctrl.soft_stop = !ctrl.soft_stop,
                                        "opening_stopped" => ctrl.opening_stopped = !ctrl.opening_stopped,
                                        "force_closing" => ctrl.force_closing = !ctrl.force_closing,
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
