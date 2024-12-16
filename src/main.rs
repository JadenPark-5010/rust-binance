// File Modules
mod order;
mod types;
mod execute_trade;
mod handle_price;
use crate::order::Order;
use handle_price::fetch_price;
use crate::types::{SharedState, SharedPrices, TradingState};

// Library
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use reqwest::Client;
use eframe::egui;
use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Default)]
struct TradingApp {
    prices: Arc<Mutex<HashMap<String, f64>>>,
    trading_state: Arc<Mutex<TradingState>>,
    last_update_time: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl TradingApp {
    fn new(prices: SharedPrices, trading_state: SharedState) -> Self {
        Self {
            prices,
            trading_state,
            last_update_time: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl eframe::App for TradingApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Crypto Trading Monitor");
            
            // 가격 정보 표시
            ui.group(|ui| {
                ui.heading("Price Information");
                if let Ok(prices) = self.prices.try_lock() {
                    let binance_price = prices.get("Binance").copied().unwrap_or_default();
                    let bitmart_price = prices.get("Bitmart").copied().unwrap_or_default();
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("Binance XRP/USDT: ${:.4}", binance_price));
                    });
                    ui.horizontal(|ui| {
                        ui.label(format!("Bitmart XRP/USDT: ${:.4}", bitmart_price));
                    });
                    
                    // 가격차 계산 및 표시
                    if binance_price > 0.0 && bitmart_price > 0.0 {
                        let price_gap = ((binance_price - bitmart_price) / bitmart_price) * 100.0;
                        ui.horizontal(|ui| {
                            ui.label(format!("Price Gap: {:.2}%", price_gap));
                        });
                    }
                }
            });

            // 포지션 정보 표시
            ui.group(|ui| {
                ui.heading("Position Information");
                if let Ok(state) = self.trading_state.try_lock() {
                    ui.horizontal(|ui| {
                        let status = if state.is_trading { "Active" } else { "Inactive" };
                        ui.label(format!("Trading Status: {}", status));
                    });
                    
                    if let Some(entry_gap) = state.entry_gap {
                        ui.horizontal(|ui| {
                            ui.label(format!("Entry Gap: {:.2}%", entry_gap));
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

            // 업데이트 요청
            ctx.request_repaint_after(Duration::from_millis(100));
        });
    }
}

#[tokio::main]
async fn main() {
    // Shared data 초기화
    let shared_prices: SharedPrices = Arc::new(Mutex::new(HashMap::new()));
    let shared_state: SharedState = Arc::new(Mutex::new(TradingState::default()));

    let client = Client::new();

    let order = Arc::new(Order {
        client: client.clone(),
        binance_api_key: "t5vKeIytVS9dSxb7rU7yHVVt8GnVE7xYYgdXfb0UQXbqTjHlFoFQx9bJmd3unHP5".to_string(),
        binance_secret_key: "XAXdTIS0tl8OHy8IBxnJyIJTbWsXiCTrBWMUuXpYR7OCVjwGxiwpRrOWo8bntMDJ".to_string(),
        bitmart_api_key: "dbc03779838b8ac83f05901ec5b416731647bc60".to_string(),
        bitmart_secret_key: "a0de4a750bcd25302ff37ae719a9d03b841a4ca84a129d790b44d49ab8eaede1".to_string(),
        bitmart_memo: "YOUR_BITMART_MEMO".to_string(),
    });

    // Binance WebSocket 처리
    let binance_shared_prices = Arc::clone(&shared_prices);
    let binance_shared_state = Arc::clone(&shared_state);
    let binance_order = Arc::clone(&order);
    tokio::spawn(fetch_price(
        "wss://fstream.binance.com/ws/xrpusdt@aggTrade",
        "Binance",
        binance_shared_prices,
        binance_shared_state,
        binance_order,
    ));

    // Bitmart WebSocket 처리
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

    // eframe GUI 실행
    let prices = Arc::clone(&shared_prices); // 'static 수명 보장
    let state = Arc::clone(&shared_state); // 'static 수명 보장

    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Crypto Trading Monitor",
        options,
        Box::new(move |_cc| Box::new(TradingApp::new(prices, state))), // move 클로저 사용
    )
    .expect("Failed to start GUI application");
}