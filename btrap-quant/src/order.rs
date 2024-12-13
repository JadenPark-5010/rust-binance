use reqwest::Client;
use serde::Deserialize;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use hex::encode;
use chrono::Utc;

type HmacSha256 = Hmac<Sha256>;

// Binance 시장가 주문 응답 구조체
#[derive(Debug, Deserialize)]
pub struct BinanceOrderResponse {
    pub symbol: String,
    pub order_id: u64, // snake_case로 변경
    pub status: String,
}

// Bitmart 시장가 주문 응답 구조체
#[derive(Debug, Deserialize)]
pub struct BitmartOrderResponse {
    pub message: String,
    pub code: i32,
}

// Order 구조체 정의
#[derive(Clone)]
pub struct Order {
    pub client: Client,
    pub binance_api_key: String,
    pub binance_secret_key: String,
    pub bitmart_api_key: String,
    pub bitmart_secret_key: String,
    pub bitmart_memo: String,
}

impl Order {
    // Binance 시장가 주문
    pub async fn place_market_order_binance(
        &self,
        symbol: &str,
        side: &str, // "BUY" or "SELL"
        quantity: f64,
    ) -> Result<BinanceOrderResponse, reqwest::Error> {
        let base_url = "https://fapi.binance.com/fapi/v1/order";
        let timestamp = Utc::now().timestamp_millis();
        let query = format!(
            "symbol={}&side={}&type=MARKET&quantity={}&timestamp={}",
            symbol, side, quantity, timestamp
        );

        let signature = self.sign_binance(&query);

        let url = format!("{}?{}&signature={}", base_url, query, signature);
        let response = self
            .client
            .post(&url)
            .header("X-MBX-APIKEY", &self.binance_api_key)
            .send()
            .await?;

        Ok(response.json::<BinanceOrderResponse>().await?)
    }

    // Bitmart 시장가 주문
    pub async fn place_market_order_bitmart(
        &self,
        symbol: &str,
        side: &str, // "buy" or "sell"
        size: f64,
    ) -> Result<BitmartOrderResponse, reqwest::Error> {
        let base_url = "https://api-cloud.bitmart.com/futures/v1/submit-order";
        let timestamp = Utc::now().timestamp_millis();
        let body = format!(
            "{{\"symbol\": \"{}\", \"side\": \"{}\", \"type\": \"market\", \"size\": {}, \"timestamp\": {}}}",
            symbol, side, size, timestamp
        );

        let signature = self.sign_bitmart(&body, timestamp);

        let response = self
            .client
            .post(base_url)
            .header("X-BM-KEY", &self.bitmart_api_key)
            .header("X-BM-SIGN", signature)
            .header("X-BM-TIMESTAMP", timestamp.to_string())
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        Ok(response.json::<BitmartOrderResponse>().await?)
    }

    // Binance 서명 생성
    fn sign_binance(&self, data: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.binance_secret_key.as_bytes()).unwrap();
        mac.update(data.as_bytes());
        encode(mac.finalize().into_bytes())
    }

    // Bitmart 서명 생성
    fn sign_bitmart(&self, body: &str, timestamp: i64) -> String {
        let payload = format!("{}{}", timestamp, body);
        let mut mac = HmacSha256::new_from_slice(self.bitmart_secret_key.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        encode(mac.finalize().into_bytes())
    }
}
