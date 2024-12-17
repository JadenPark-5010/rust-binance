// File Modules
mod order;
mod types;
mod execute_trade;
mod handle_price;
use crate::order::Order;
use handle_price::fetch_price;
use crate::types::{SharedState, SharedPrices, TradingState};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;

// Library
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use reqwest::Client;
use eframe::egui;
use chrono::{DateTime, FixedOffset, Utc};
use std::time::Duration;

// KST 변환 함수
fn get_kst_time() -> String {
    let kst_offset = FixedOffset::east_opt(9 * 3600).unwrap(); // UTC+9 (KST)
    let now_utc: DateTime<Utc> = Utc::now();
    let now_kst = now_utc.with_timezone(&kst_offset);
    now_kst.format("%Y-%m-%d %H:%M:%S%.3f").to_string() // 밀리초 단위로 포맷
}

#[derive(Debug, serde::Deserialize)]
pub struct DepthAllItem {
    pub price: String, // 가격
    pub vol: String,   // 수량
}

#[derive(Debug, serde::Deserialize)]
pub struct DepthAllData {
    pub symbol: String,
    pub asks: Vec<DepthAllItem>, // 매도 호가
    pub bids: Vec<DepthAllItem>, // 매수 호가
    pub ms_t: u64,               // 타임스탬프
}

#[derive(Debug, serde::Deserialize)]
pub struct DepthAllResponse {
    pub data: DepthAllData,
    pub group: String,
}

fn calculate_max_position_value(depth: &Vec<DepthAllItem>, base_price: f64, tolerance: f64) -> f64 {
    let limit_price = base_price * (1.0 + tolerance); // 슬리피지 허용 한계
    let mut total_value = 0.0;

    for entry in depth {
        let price: f64 = entry.price.parse().unwrap_or(0.0);
        let volume: f64 = entry.vol.parse().unwrap_or(0.0);

        if price > limit_price {
            break; // 슬리피지를 초과하면 종료
        }
        total_value += price * volume; // 가격 * 수량
    }

    total_value / 5.0 // 레버리지 5배 적용
}


pub async fn fetch_bitmart_depth(
    url: &str,
    market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>>,
) {
    let (ws_stream, _) = connect_async(url).await.expect("WebSocket 연결 실패");
    let (mut write, mut read) = ws_stream.split();

    let subscribe_message = r#"{"action": "subscribe", "args": ["futures/depthAll20:SOLUSDT@100ms"]}"#;
    write
        .send(tokio_tungstenite::tungstenite::Message::Text(subscribe_message.into()))
        .await
        .unwrap();

    println!("BitMart Market Depth WebSocket Connected");

    while let Some(message) = read.next().await {
        if let Ok(text) = message.unwrap().to_text() {

            // JSON 파싱
            if let Ok(response) = serde_json::from_str::<DepthAllResponse>(text) {
                let mut depth = market_depth.lock().await;

                if let Some(data) = Some(response.data) {
                    depth.insert("BitMart_Asks".to_string(), data.asks);
                    depth.insert("BitMart_Bids".to_string(), data.bids);
                }
            } else {
                println!("JSON 파싱 실패: {}", text);
            }
        }
    }
}

#[derive(Default)]
struct TradingApp {
    prices: Arc<Mutex<HashMap<String, f64>>>,
    market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>>,
    trading_state: Arc<Mutex<TradingState>>,
    last_update_time: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl TradingApp {
    fn new(
        prices: SharedPrices,
        market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>>,
        trading_state: SharedState,
    ) -> Self {
        Self {
            prices,
            market_depth,
            trading_state,
            last_update_time: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl eframe::App for TradingApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Crypto Trading Monitor");
            
            ui.group(|ui| {
                ui.heading("Current Time (KST)");
                let kst_time = get_kst_time();
                ui.label(format!("KST: {}", kst_time));
            });

            // Price Information
            ui.group(|ui| {
                ui.heading("Price Information");
                if let Ok(prices) = self.prices.try_lock() {
                    let binance_price = prices.get("Binance").copied().unwrap_or_default();
                    let bitmart_price = prices.get("Bitmart").copied().unwrap_or_default();
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("Binance SOL/USDT: ${:.4}", binance_price));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("Bitmart SOL/USDT: ${:.4}", bitmart_price));
                    });
                    
                    if binance_price > 0.0 && bitmart_price > 0.0 {
                        let price_gap = ((binance_price - bitmart_price) / bitmart_price) * 100.0;
                        ui.horizontal(|ui| {
                            ui.label(format!("Price Gap: {:.4}%", price_gap));
                        });
                    }
                }
            });

            // Slippage-Based Position Value
            ui.group(|ui| {
                ui.heading("Slippage Information");
            
                if let Ok(depth) = self.market_depth.try_lock() {
                    let mut long_value = 0.0;
                    let mut short_value = 0.0;
            
                    if let Some(asks) = depth.get("BitMart_Asks") {
                        if let Some(best_ask) = asks.first() {
                            let best_ask_price: f64 = best_ask.price.parse().unwrap_or(0.0);
                            long_value = calculate_max_position_value(asks, best_ask_price, 0.00025);
            
                            ui.horizontal(|ui| {
                                ui.label(format!("Best Ask Price: ${:.2}", best_ask_price));
                                ui.label(format!("Max Long Position Value (0.1% Slippage, Leverage x5): ${:.2}", long_value));
                            });
                        }
                    } else {
                        ui.label("No Asks Data Available");
                    }
            
                    if let Some(bids) = depth.get("BitMart_Bids") {
                        if let Some(best_bid) = bids.first() {
                            let best_bid_price: f64 = best_bid.price.parse().unwrap_or(0.0);
                            short_value = calculate_max_position_value(bids, best_bid_price, 0.00025);
            
                            ui.horizontal(|ui| {
                                ui.label(format!("Best Bid Price: ${:.2}", best_bid_price));
                                ui.label(format!("Max Short Position Value (0.1% Slippage, Leverage x5): ${:.2}", short_value));
                            });
                        }
                    } else {
                        ui.label("No Bids Data Available");
                    }
                } else {
                    ui.label("No Market Depth Data Available");
                }
            });
            

            // Position Information
            ui.group(|ui| {
                ui.heading("Position Information");
                if let Ok(state) = self.trading_state.try_lock() {
                    ui.horizontal(|ui| {
                        let status = if state.is_trading { "Active" } else { "Inactive" };
                        ui.label(format!("Trading Status: {}", status));
                    });
                    
                    if let Some(entry_gap) = state.entry_gap {
                        ui.horizontal(|ui| {
                            ui.label(format!("Entry Gap: {:.4}%", entry_gap));
                        });
                    }

                    if let Some(position) = &state.binance_position {
                        ui.horizontal(|ui| {
                            ui.label(format!("Binance Position: {}", position));
                        });
                    }

                    if let Some(position) = &state.bitmart_position {
                        ui.horizontal(|ui| {
                            ui.label(format!("Bitmart Position: {}", position));
                        });
                    }

                    if let Some(open_time) = state.position_open_time {
                        ui.horizontal(|ui| {
                            let duration = Utc::now().signed_duration_since(open_time);
                            let minutes = duration.num_minutes();
                            ui.label(format!("Position Duration: {}m", minutes));
                        });
                    }
                }
            });

            ctx.request_repaint_after(Duration::from_millis(100));
        });
    }
}

#[tokio::main]
async fn main() {
    // 공유 데이터 초기화
    let shared_prices: SharedPrices = Arc::new(Mutex::new(HashMap::new()));
    let shared_state: SharedState = Arc::new(Mutex::new(TradingState::default()));
    let shared_market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>> = Arc::new(Mutex::new(HashMap::new()));

    let client = Client::new();

    // 주문 객체 생성
    let order = Arc::new(Order {
        client: client.clone(),
        binance_api_key: "BBhJXZ8MhulWTNkniWRdS1GhHiWSNXOJz71cOQPAcHQ4jYHKQ7XmxUK4yslcvcSF".to_string(),
        binance_secret_key: "OEzUXj3jYzscWqIiZeHC7MA79f0TG1kBor7N3CSBYdEdHcwFxheR2mAqjJnUox2j".to_string(),
        bitmart_api_key: "dbc03779838b8ac83f05901ec5b416731647bc60".to_string(),
        bitmart_secret_key: "a0de4a750bcd25302ff37ae719a9d03b841a4ca84a129d790b44d49ab8eaede1".to_string(),
        bitmart_memo: "bitmart-arbitrage".to_string(),
    });

    // Binance 가격 수신 스레드
    let binance_shared_prices = Arc::clone(&shared_prices);
    let binance_shared_state = Arc::clone(&shared_state);
    let binance_order = Arc::clone(&order);
    tokio::spawn(fetch_price(
        "wss://fstream.binance.com/ws/solusdt@aggTrade",
        "Binance",
        binance_shared_prices,
        binance_shared_state,
        binance_order,
    ));

    // BitMart 가격 수신 스레드
    let bitmart_shared_prices = Arc::clone(&shared_prices);
    let bitmart_shared_state = Arc::clone(&shared_state);
    let bitmart_order = Arc::clone(&order);
    tokio::spawn(fetch_price(
        "wss://openapi-ws-v2.bitmart.com/api?protocol=1.1",
        "Bitmart",
        bitmart_shared_prices,
        bitmart_shared_state,
        bitmart_order,
    ));

    // BitMart Market Depth 수신 스레드
    let bitmart_market_depth = Arc::clone(&shared_market_depth);
    tokio::spawn(fetch_bitmart_depth(
        "wss://openapi-ws-v2.bitmart.com/api?protocol=1.1",
        bitmart_market_depth,
    ));

    // GUI 애플리케이션 실행
    let prices = Arc::clone(&shared_prices);
    let market_depth = Arc::clone(&shared_market_depth);
    let state = Arc::clone(&shared_state);

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Cross Exchange Arbitrage Trading Monitor",
        options,
        Box::new(move |_cc| Box::new(TradingApp::new(prices, market_depth, state))),
    )
    .expect("Failed to start GUI application");
}
