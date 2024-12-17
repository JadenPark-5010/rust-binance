use crate::types::{SharedState, SharedPrices};
use crate::execute_trade::{execute_trade};
use futures_util::{stream::StreamExt, SinkExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::Value;
use std::sync::Arc;

use crate::order::Order;

async fn handle_price_update(
    exchange_name: &str,
    new_price: f64,
    shared_prices: &SharedPrices,
    shared_state: SharedState,
    order: Arc<Order>,
) {
    let mut prices = shared_prices.lock().await;
    prices.insert(exchange_name.to_string(), new_price);

    if let (Some(&binance_price), Some(&bitmart_price)) = (prices.get("Binance"), prices.get("Bitmart")) {
        execute_trade(order.clone(), binance_price, bitmart_price, shared_state).await;
    }
}

pub async fn fetch_price(
    websocket_url: &str,
    exchange_name: &str,
    shared_prices: SharedPrices,
    shared_state: SharedState,
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
                    "args": ["futures/trade:SOLUSDT"]
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
                                            handle_price_update(exchange_name, new_price, &shared_prices, Arc::clone(&shared_state), order.clone()).await;
                                        }
                                    }
                                } else if exchange_name == "Bitmart" {
                                    if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
                                        for entry in data {
                                            if let Some(price_str) = entry.get("deal_price").and_then(|v| v.as_str()) {
                                                if let Ok(new_price) = price_str.parse::<f64>() {
                                                    handle_price_update(exchange_name, new_price, &shared_prices, Arc::clone(&shared_state), order.clone()).await;
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