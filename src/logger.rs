use chrono::Local;
use colored::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{Arc, OnceLock};

use crate::web::WebState;

/// 全局 WebState 引用
static WEB_STATE: OnceLock<Arc<WebState>> = OnceLock::new();

pub struct Logger {
    level: LogLevel,
    file: Option<Mutex<std::fs::File>>,
    tlog_timestamps: Mutex<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace, Debug, Info, Warn, Error,
}

impl LogLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "TRACE" => LogLevel::Trace, "DEBUG" => LogLevel::Debug,
            "INFO" => LogLevel::Info, "WARN" | "WARNING" => LogLevel::Warn,
            "ERROR" => LogLevel::Error, _ => LogLevel::Info,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE", LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ", LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }
}

impl Logger {
    pub fn new(level: &str, file_path: Option<&str>) -> Self {
        let file = file_path.and_then(|p| {
            if let Some(parent) = std::path::Path::new(p).parent() {
                let _ = fs::create_dir_all(parent);
            }
            OpenOptions::new().create(true).append(true).open(p).ok().map(Mutex::new)
        });
        Logger { level: LogLevel::from_str(level), file, tlog_timestamps: Mutex::new(HashMap::new()) }
    }

    /// 绑定 WebState — 可在任意时刻调用
    pub fn bind_web_state(state: Arc<WebState>) {
        let _ = WEB_STATE.set(state);
    }

    pub fn log(&self, msg: &str, level: &str, color: Option<&str>) {
        let log_level = LogLevel::from_str(level);
        if log_level < self.level { return; }

        let now = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let level_str = log_level.as_str();

        let level_colored = match log_level {
            LogLevel::Trace => level_str.dimmed(), LogLevel::Debug => level_str.cyan(),
            LogLevel::Info => level_str.green(), LogLevel::Warn => level_str.yellow(),
            LogLevel::Error => level_str.red(),
        };
        let msg_colored = match color {
            Some("green") => msg.green().to_string(), Some("red") => msg.red().to_string(),
            Some("blue") => msg.blue().to_string(), Some("yellow") => msg.yellow().to_string(),
            Some("cyan") => msg.cyan().to_string(), Some("magenta") => msg.magenta().to_string(),
            _ => msg.to_string(),
        };

        println!("[{}] [{}] {}", now, level_colored, msg_colored);

        if let Some(ref file) = self.file {
            let line = format!("[{}] [{}] {}\n", now, level_str, msg);
            let _ = file.lock().write_all(line.as_bytes());
        }

        if let Some(web) = WEB_STATE.get() {
            web.push_log(msg, level_str.trim(), color.unwrap_or(""));
        }
    }

    pub fn tlog(&self, tag: &str, msg: &str, color: Option<&str>, interval: f64, level: &str, _query: bool) -> bool {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
        let mut timestamps = self.tlog_timestamps.lock();
        let last = timestamps.get(tag).copied().unwrap_or(0.0);
        if now - last < interval { return false; }
        timestamps.insert(tag.to_string(), now);
        self.log(msg, level, color);
        true
    }
}
