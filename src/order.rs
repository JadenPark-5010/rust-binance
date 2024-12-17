use reqwest::Client;
use serde::Deserialize;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use hex::encode;
use chrono::Utc;
use serde_json::{json, Value};
type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
pub struct BinanceOrderResponse {
    pub symbol: String,
    pub orderId: u64,
    pub avgPrice: String,
}

#[derive(Debug, Deserialize)]
pub struct BitmartOrderResponse {
    pub message: String,
    pub code: i32,
    pub data: Value
}

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
    pub async fn place_market_order_binance(
        &self,
        symbol: &str,
        side: &str,
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

    pub async fn place_market_order_bitmart(
        &self,
        symbol: &str,
        side: i32,
        size: i32,
    ) -> Result<BitmartOrderResponse, reqwest::Error> {
        let base_url = "https://api-cloud-v2.bitmart.com/contract/private/submit-order";
        let timestamp = Utc::now().timestamp_millis();
        let body = json!({
            "symbol": symbol,
            "side": side,
            "type": "market",
            "size": size,
            "leverage": "5",
            "open_type": "isolated"
        });
        let body_string = body.to_string();
        let query_string = "";
        let signature = self.sign_bitmart(query_string, &body_string, timestamp);

        let response = self
            .client
            .post(base_url)
            .header("X-BM-KEY", &self.bitmart_api_key)
            .header("X-BM-SIGN", signature)
            .header("X-BM-TIMESTAMP", timestamp.to_string())
            .header("Content-Type", "application/json")
            .body(body_string)
            .send()
            .await?;
        
        
        Ok(response.json::<BitmartOrderResponse>().await?)
    }

    fn sign_binance(&self, data: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.binance_secret_key.as_bytes()).unwrap();
        mac.update(data.as_bytes());
        encode(mac.finalize().into_bytes())
    }

    // Bitmart 서명 생성
    fn sign_bitmart(&self, query_string: &str, body: &str, timestamp: i64) -> String {
        let payload = format!(
            "{}#{}#{}",
            timestamp,
            self.bitmart_memo,
            body
        );
    
        let mut mac = HmacSha256::new_from_slice(self.bitmart_secret_key.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
    
        encode(mac.finalize().into_bytes())
    }
}
