use crate::types::{SharedState, SharedPrices};
use crate::order::Order;
use crate::DepthAllItem;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct MarketPrice {
    pub long_price: f64,
    pub short_price: f64,
}

pub struct PriceCalculator {
    shared_market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>>,
    shared_prices: SharedPrices,
    shared_state: SharedState,
    order: Arc<Order>,
    position_size: f64,
}

impl PriceCalculator {
    pub fn new(
        shared_market_depth: Arc<Mutex<HashMap<String, Vec<DepthAllItem>>>>,
        shared_prices: SharedPrices,
        shared_state: SharedState,
        order: Arc<Order>,
        position_size: f64,
    ) -> Self {
        Self {
            shared_market_depth,
            shared_prices,
            shared_state,
            order,
            position_size,
        }
    }

    pub async fn calculate_execution_price(
        depth: &Vec<DepthAllItem>,
        position_size: f64,
    ) -> f64 {
        let mut remaining_amount = position_size;
        let mut total_cost = 0.0;
        let mut total_quantity = 0.0;

        for item in depth.iter() {
            let price: f64 = item.price.parse().unwrap_or(0.0);
            let volume: f64 = item.vol.parse().unwrap_or(0.0);
            
            let available_quantity = if remaining_amount > price * volume {
                volume
            } else {
                remaining_amount / price
            };

            total_cost += price * available_quantity;
            total_quantity += available_quantity;
            remaining_amount -= price * available_quantity;

            if remaining_amount <= 0.0 {
                break;
            }
        }

        if total_quantity > 0.0 {
            total_cost / total_quantity
        } else {
            0.0
        }
    }

    pub async fn update_market_prices(&self) -> (MarketPrice, MarketPrice) {
        let market_depth = self.shared_market_depth.lock().await;
        let mut prices = self.shared_prices.lock().await;

        let bitmart_asks = market_depth.get("BitMart_Asks").cloned().unwrap_or_default();
        let bitmart_bids = market_depth.get("BitMart_Bids").cloned().unwrap_or_default();
        
        let bitmart_long_price = Self::calculate_execution_price(&bitmart_asks, self.position_size).await;
        let bitmart_short_price = Self::calculate_execution_price(&bitmart_bids, self.position_size).await;

        // 임시로 Binance 가격을 spread로 계산
        let binance_base_price = prices.get("Binance").copied().unwrap_or_default();
        let binance_long_price = binance_base_price * 1.0005;
        let binance_short_price = binance_base_price * 0.9995;

        let binance_prices = MarketPrice {
            long_price: binance_long_price,
            short_price: binance_short_price,
        };

        let bitmart_prices = MarketPrice {
            long_price: bitmart_long_price,
            short_price: bitmart_short_price,
        };

        prices.insert("Binance_Long".to_string(), binance_long_price);
        prices.insert("Binance_Short".to_string(), binance_short_price);
        prices.insert("Bitmart_Long".to_string(), bitmart_long_price);
        prices.insert("Bitmart_Short".to_string(), bitmart_short_price);

        (binance_prices, bitmart_prices)
    }

    pub async fn check_and_execute_arbitrage(&self) {
        let mut state = self.shared_state.lock().await;
        
        let (binance_prices, bitmart_prices) = self.update_market_prices().await;
        
        let gap1 = (binance_prices.short_price - bitmart_prices.long_price) / bitmart_prices.long_price * 100.0;
        let gap2 = (bitmart_prices.short_price - binance_prices.long_price) / binance_prices.long_price * 100.0;

        if gap1 > 0.3 && !state.is_trading {
            println!("Executing Arbitrage: Binance Short - Bitmart Long, Gap: {:.4}%", gap1);
            state.is_trading = true;
            state.entry_gap = Some(gap1);
            state.position_open_time = Some(Utc::now());
            // TODO: Implement trade execution
        } else if gap2 > 0.3 && !state.is_trading {
            println!("Executing Arbitrage: Bitmart Short - Binance Long, Gap: {:.4}%", gap2);
            state.is_trading = true;
            state.entry_gap = Some(gap2);
            state.position_open_time = Some(Utc::now());
            // TODO: Implement trade execution
        }
    }
}