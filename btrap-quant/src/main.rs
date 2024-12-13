use futures_util::{stream::StreamExt, SinkExt}; // StreamExt 및 SinkExt 가져오기
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use std::time::SystemTime;

async fn fetch_price(websocket_url: &str, exchange_name: &str) {
    println!("Connecting to {} WebSocket...", exchange_name);

    match connect_async(websocket_url).await {
        Ok((ws_stream, _)) => {
            println!("Connected to {} WebSocket.", exchange_name);

            let (mut write, mut read) = ws_stream.split(); // Stream 분리

            // Bitmart의 경우 구독 메시지를 전송
            if exchange_name == "Bitmart" {
                let sub_msg = serde_json::json!({
                    "action": "subscribe",
                    "args": ["futures/trade:BTCUSDT"]
                });
                println!("Sending subscription message to Bitmart: {}", sub_msg);
                if let Err(e) = write.send(Message::Text(sub_msg.to_string())).await {
                    eprintln!("Failed to send subscription message to {}: {}", exchange_name, e);
                    return;
                }

                // 구독 메시지에 대한 응답 확인
                if let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            println!("Subscription response from Bitmart: {}", text);
                        }
                        Ok(other) => {
                            println!("Unexpected response from Bitmart: {:?}", other);
                        }
                        Err(e) => {
                            eprintln!("Error reading subscription response from Bitmart: {}", e);
                        }
                    }
                }
            }

            // WebSocket 메시지 처리
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<Value>(&text) {
                            Ok(json) => {
                                if exchange_name == "Binance" {
                                    if let Some(price) = json.get("p").and_then(|v| v.as_str()) {
                                        println!(
                                            "[{}] {} Price: {} USDT",
                                            SystemTime::now()
                                                .duration_since(SystemTime::UNIX_EPOCH)
                                                .unwrap()
                                                .as_secs(),
                                            exchange_name,
                                            price
                                        );
                                    }
                                } else if exchange_name == "Bitmart" {
                                    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
                                        for entry in data {
                                            if let Some(deal_price) = entry.get("deal_price").and_then(|v| v.as_str()) {
                                                println!(
                                                    "[{}] {} Price: {} USDT",
                                                    SystemTime::now()
                                                        .duration_since(SystemTime::UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_secs(),
                                                    exchange_name,
                                                    deal_price
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error parsing JSON from {}: {}", exchange_name, e);
                            }
                        }
                    }
                    Ok(Message::Ping(payload)) => {
                        println!("Ping received from {}.", exchange_name);
                        write.send(Message::Pong(payload)).await.unwrap();
                    }
                    Ok(Message::Close(_)) => {
                        println!("Connection closed by {}.", exchange_name);
                        break;
                    }
                    Err(e) => {
                        eprintln!("WebSocket error from {}: {}", exchange_name, e);
                        break;
                    }
                    _ => {}
                }
            }

            println!("WebSocket connection closed for {}.", exchange_name);
        }
        Err(e) => {
            eprintln!("Failed to connect to {} WebSocket: {}", exchange_name, e);
        }
    }
}

#[tokio::main]
async fn main() {
    let binance_url = "wss://fstream.binance.com/ws/btcusdt@aggTrade";
    let bitmart_url = "wss://openapi-ws-v2.bitmart.com/api?protocol=1.1";

    // Binance WebSocket
    tokio::spawn(fetch_price(binance_url, "Binance"));

    // Bitmart WebSocket
    tokio::spawn(fetch_price(bitmart_url, "Bitmart"));

    // Keep the main task alive
    tokio::signal::ctrl_c().await.unwrap();
    println!("Shutting down...");
}

