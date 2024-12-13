use futures_util::{stream::StreamExt, SinkExt}; // StreamExt 및 SinkExt 가져오기
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use std::time::SystemTime;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

type SharedPrices = Arc<Mutex<HashMap<String, f64>>>;

async fn handle_price_update(
    exchange_name: &str,
    new_price: f64,
    shared_prices: &SharedPrices,
) {
    let mut prices = shared_prices.lock().unwrap(); // Mutex 잠금

    // 현재 거래소 가격 업데이트
    prices.insert(exchange_name.to_string(), new_price);

    // 두 거래소의 가격 비교
    if let (Some(binance_price), Some(bitmart_price)) = (prices.get("Binance"), prices.get("Bitmart")) {
        let percent_diff = if exchange_name == "Binance" {
            ((new_price - bitmart_price) / bitmart_price) * 100.0
        } else {
            ((new_price - binance_price) / binance_price) * 100.0
        };

        // 현재 시간 가져오기
        let now = SystemTime::now();
        let datetime: DateTime<Utc> = DateTime::<Utc>::from(now);

        println!(
            "[{}] {} Price: {:.4} USDT, Difference: {:.4}%",
            datetime.to_rfc3339(),
            exchange_name,
            new_price,
            percent_diff
        );
    }
}

async fn fetch_price(
    websocket_url: &str,
    exchange_name: &str,
    shared_prices: SharedPrices, // 공유 데이터 구조 추가
) {
    println!("Connecting to {} WebSocket...", exchange_name);

    match connect_async(websocket_url).await {
        Ok((ws_stream, _)) => {
            println!("Connected to {} WebSocket.", exchange_name);

            let (mut write, mut read) = ws_stream.split();

            if exchange_name == "Bitmart" {
                let sub_msg = serde_json::json!({
                    "action": "subscribe",
                    "args": ["futures/trade:XRPUSDT"]
                });
                if let Err(e) = write.send(Message::Text(sub_msg.to_string())).await {
                    eprintln!("Failed to send subscription message to {}: {}", exchange_name, e);
                    return;
                }
            }

            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<Value>(&text) {
                            Ok(json) => {
                                if exchange_name == "Binance" {
                                    if let Some(price_str) = json.get("p").and_then(|v| v.as_str()) {
                                        if let Ok(new_price) = price_str.parse::<f64>() {
                                            handle_price_update(
                                                exchange_name,
                                                new_price,
                                                &shared_prices,
                                            )
                                            .await;
                                        }
                                    }
                                } else if exchange_name == "Bitmart" {
                                    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
                                        for entry in data {
                                            if let Some(price_str) = entry.get("deal_price").and_then(|v| v.as_str()) {
                                                if let Ok(new_price) = price_str.parse::<f64>() {
                                                    handle_price_update(
                                                        exchange_name,
                                                        new_price,
                                                        &shared_prices,
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => eprintln!("Error parsing JSON from {}: {}", exchange_name, e),
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        write.send(Message::Pong(payload)).await.unwrap();
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        eprintln!("WebSocket error from {}: {}", exchange_name, e);
                        break;
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to {} WebSocket: {}", exchange_name, e);
        }
    }
}

#[tokio::main]
async fn main() {
    let binance_url = "wss://fstream.binance.com/ws/xrpusdt@aggTrade";
    let bitmart_url = "wss://openapi-ws-v2.bitmart.com/api?protocol=1.1";

    // 공유 데이터 구조 생성
    let shared_prices: SharedPrices = Arc::new(Mutex::new(HashMap::new()));

    // Binance WebSocket
    let binance_shared = Arc::clone(&shared_prices);
    tokio::spawn(fetch_price(binance_url, "Binance", binance_shared));

    // Bitmart WebSocket
    let bitmart_shared = Arc::clone(&shared_prices);
    tokio::spawn(fetch_price(bitmart_url, "Bitmart", bitmart_shared));

    // Keep the main task alive
    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down...");
}