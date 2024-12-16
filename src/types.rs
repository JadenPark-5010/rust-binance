use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub type SharedState = Arc<Mutex<TradingState>>;
pub type SharedPrices = Arc<Mutex<HashMap<String, f64>>>;

#[derive(Default)]
pub struct TradingState {
    pub is_trading: bool,
    pub entry_gap: Option<f64>,
    pub binance_position: Option<String>,
    pub bitmart_position: Option<String>,
    pub position_open_time: Option<DateTime<Utc>>,
}