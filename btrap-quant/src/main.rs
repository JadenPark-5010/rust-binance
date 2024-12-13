use futures_util::{stream::StreamExt, SinkExt}; // StreamExt 및 SinkExt 가져오기
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use std::sync::{Arc};
use std::collections::HashMap;
use tokio::sync::Mutex;
use reqwest::Client;
mod order;
use crate::order::Order; // Import the Order module

// 공유 데이터 타입 정의
type SharedPrices = Arc<Mutex<HashMap<String, f64>>>;

// 주문 집행 함수 (실제 주문 실행)
async fn execute_trade(
    order: Arc<Order>,
    binance_price: f64,
    bitmart_price: f64,
) {
    let percent_diff = ((binance_price - bitmart_price) / bitmart_price) * 100.0;

    if percent_diff > 0.3 {
        println!(
            "Gap exceeds 0.3%. Executing trade: Binance Short, Bitmart Long.\nBinance: {:.4}, Bitmart: {:.4}, Gap: {:.4}%",
            binance_price, bitmart_price, percent_diff
        );
        // Binance 숏 주문
        match order.place_market_order_binance("XRPUSDT", "SELL", 1.0).await {
            Ok(response) => println!("[Order] Binance Short Order Response: {:?}", response),
            Err(e) => eprintln!("[Order] Binance Short Order Failed: {}", e),
        }
        // Bitmart 롱 주문
        match order.place_market_order_bitmart("XRPUSDT", "buy", 1.0).await {
            Ok(response) => println!("[Order] Bitmart Long Order Response: {:?}", response),
            Err(e) => eprintln!("[Order] Bitmart Long Order Failed: {}", e),
        }
    } else if percent_diff < -0.3 {
        println!(
            "Gap exceeds -0.3%. Executing trade: Binance Long, Bitmart Short.\nBinance: {:.4}, Bitmart: {:.4}, Gap: {:.4}%",
            binance_price, bitmart_price, percent_diff
        );
        // Binance 롱 주문
        match order.place_market_order_binance("XRPUSDT", "BUY", 1.0).await {
            Ok(response) => println!("[Order] Binance Long Order Response: {:?}", response),
            Err(e) => eprintln!("[Order] Binance Long Order Failed: {}", e),
        }
        // Bitmart 숏 주문
        match order.place_market_order_bitmart("XRPUSDT", "sell", 1.0).await {
            Ok(response) => println!("[Order] Bitmart Short Order Response: {:?}", response),
            Err(e) => eprintln!("[Order] Bitmart Short Order Failed: {}", e),
        }
    }
}

// 가격 업데이트 핸들러
async fn handle_price_update(
    exchange_name: &str,
    new_price: f64,
    shared_prices: &SharedPrices,
    order: Arc<Order>,
) {
    let mut prices = shared_prices.lock().await; // 비동기 Mutex 잠금

    // 현재 거래소 가격 업데이트
    prices.insert(exchange_name.to_string(), new_price);

    // 두 거래소의 가격 비교
    if let (Some(&binance_price), Some(&bitmart_price)) = (prices.get("Binance"), prices.get("Bitmart")) {
        // 주문 조건 확인 및 실행
        execute_trade(order.clone(), binance_price, bitmart_price).await;
    }
}

// WebSocket에서 가격 가져오기
async fn fetch_price(
    websocket_url: &str,
    exchange_name: &str,
    shared_prices: SharedPrices, // 공유 데이터 구조 추가
    order: Arc<Order>,
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
                                            handle_price_update(exchange_name, new_price, &shared_prices, order.clone()).await;
                                        }
                                    }
                                } else if exchange_name == "Bitmart" {
                                    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
                                        for entry in data {
                                            if let Some(price_str) = entry.get("deal_price").and_then(|v| v.as_str()) {
                                                if let Ok(new_price) = price_str.parse::<f64>() {
                                                    handle_price_update(exchange_name, new_price, &shared_prices, order.clone()).await;
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

    // HTTP 클라이언트 생성
    let client = Client::new();

    // Order 구조체 생성
    let order = Arc::new(Order {
        client: client.clone(),
        binance_api_key: "YOUR_BINANCE_API_KEY".to_string(),
        binance_secret_key: "YOUR_BINANCE_SECRET_KEY".to_string(),
        bitmart_api_key: "YOUR_BITMART_API_KEY".to_string(),
        bitmart_secret_key: "YOUR_BITMART_SECRET_KEY".to_string(),
        bitmart_memo: "YOUR_BITMART_MEMO".to_string(),
    });

    // Binance WebSocket
    let binance_shared = Arc::clone(&shared_prices);
    let binance_order = Arc::clone(&order);
    tokio::spawn(fetch_price(binance_url, "Binance", binance_shared, binance_order));

    // Bitmart WebSocket
    let bitmart_shared = Arc::clone(&shared_prices);
    let bitmart_order = Arc::clone(&order);
    tokio::spawn(fetch_price(bitmart_url, "Bitmart", bitmart_shared, bitmart_order));

    // Keep the main task alive
    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down...");
}
