use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::Engine as _;

type HmacSha256 = Hmac<Sha256>;

/// Bitget v2 REST API 签名
///
/// 签名公式: HMAC-SHA256(secret, timestamp + METHOD + path[?query] + body)
/// 输出: Base64 编码
///
/// 示例:
///   timestamp = "1234567890123"
///   method = "POST"
///   path = "/api/v2/mix/order/place-order"
///   body = "{\"symbol\":\"BTCUSDT\"}"
///   pre_hash = "1234567890123POST/api/v2/mix/order/place-order{\"symbol\":\"BTCUSDT\"}"
pub fn sign_request(secret: &str, timestamp: &str, method: &str, path: &str, query: &str, body: &str) -> String {
    let pre_hash = if query.is_empty() {
        format!("{}{}{}{}", timestamp, method.to_uppercase(), path, body)
    } else {
        format!("{}{}{}?{}{}", timestamp, method.to_uppercase(), path, query, body)
    };
    hmac_sha256_base64(secret, &pre_hash)
}

/// Bitget v2 WebSocket 登录签名
///
/// 签名公式: HMAC-SHA256(secret, timestamp + "GET/user/verify")
/// 输出: Base64 编码
///
/// timestamp 用秒级 (不是毫秒)
pub fn sign_ws_login(secret: &str, timestamp: &str) -> String {
    let pre_hash = format!("{}GET/user/verify", timestamp);
    hmac_sha256_base64(secret, &pre_hash)
}

/// HMAC-SHA256 + Base64 编码
pub fn hmac_sha256_base64(secret: &str, message: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_request() {
        let secret = "test_secret";
        let ts = "1234567890123";
        let sig = sign_request(secret, ts, "GET", "/api/v2/mix/account/accounts", "productType=USDT-FUTURES", "");
        assert!(!sig.is_empty());
        // 带 body
        let sig2 = sign_request(secret, ts, "POST", "/api/v2/mix/order/place-order", "", "{\"symbol\":\"BTCUSDT\"}");
        assert!(!sig2.is_empty());
        assert_ne!(sig, sig2);
    }

    #[test]
    fn test_sign_ws_login() {
        let secret = "test_secret";
        let ts = "1234567890";
        let sig = sign_ws_login(secret, ts);
        assert!(!sig.is_empty());
    }
}
