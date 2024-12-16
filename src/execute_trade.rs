use std::sync::Arc;
use crate::order::Order;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use std::time::SystemTime;
use crate::types::{SharedState};

async fn log_event(message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let log_message = format!("[{}] {}\n", timestamp, message);

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open("trading_log.txt")
        .await
        .unwrap();

    if let Err(e) = file.write_all(log_message.as_bytes()).await {
        eprintln!("Failed to write to log file: {}", e);
    }
}

pub async fn execute_trade(
    order: Arc<Order>,
    binance_price: f64,
    bitmart_price: f64,
    shared_state: SharedState,
) {
    let mut state = shared_state.lock().await;
    let percent_diff = ((binance_price - bitmart_price) / bitmart_price) * 100.0;

    if !state.is_trading && percent_diff.abs() > 0.01 {
        let entry_message = format!(
            "Gap exceeds 0.01%. Executing trade.\nBinance: {:.4}, Bitmart: {:.4}, Gap: {:.4}%",
            binance_price, bitmart_price, percent_diff
        );
        println!("{}", entry_message);
        log_event(&entry_message).await;

        state.is_trading = true;
        state.entry_gap = Some(percent_diff);

        if percent_diff > 0.01 {
            state.binance_position = Some("SHORT".to_string());
            state.bitmart_position = Some("LONG".to_string());

            match order.place_market_order_binance("XRPUSDT", "SELL", 100.0).await {
                Ok(response) => {
                    let message = format!("[Order] Binance Short Order Response: {:?}", response);
                    println!("{}", message);
                    log_event(&message).await;
                }
                Err(e) => eprintln!("[Order] Binance Short Order Failed: {}", e),
            }
            match order.place_market_order_bitmart("XRPUSDT", "buy", 100.0).await {
                Ok(response) => {
                    let message = format!("[Order] Bitmart Long Order Response: {:?}", response);
                    println!("{}", message);
                    log_event(&message).await;
                }
                Err(e) => eprintln!("[Order] Bitmart Long Order Failed: {}", e),
            }
        } else if percent_diff < -0.01 {
            state.binance_position = Some("LONG".to_string());
            state.bitmart_position = Some("SHORT".to_string());

            match order.place_market_order_binance("XRPUSDT", "BUY", 100.0).await {
                Ok(response) => {
                    let message = format!("[Order] Binance Long Order Response: {:?}", response);
                    println!("{}", message);
                    log_event(&message).await;
                }
                Err(e) => eprintln!("[Order] Binance Long Order Failed: {}", e),
            }
            match order.place_market_order_bitmart("XRPUSDT", "sell", 100.0).await {
                Ok(response) => {
                    let message = format!("[Order] Bitmart Short Order Response: {:?}", response);
                    println!("{}", message);
                    log_event(&message).await;
                }
                Err(e) => eprintln!("[Order] Bitmart Short Order Failed: {}", e),
            }
        }
    } else if state.is_trading {
        if let Some(entry_gap) = state.entry_gap {
            let gap_reduction = (entry_gap - percent_diff).abs();

            if gap_reduction >= 0.01 {
                let close_message = format!(
                    "Closing positions. Entry Gap: {:.4}%, Current Gap: {:.4}%, Reduction: {:.4}%",
                    entry_gap, percent_diff, gap_reduction
                );
                println!("{}", close_message);
                log_event(&close_message).await;

                if let (Some(binance_position), Some(bitmart_position)) =
                    (&state.binance_position, &state.bitmart_position)
                {
                    if binance_position == "SHORT" {
                        match order.place_market_order_binance("XRPUSDT", "BUY", 100.0).await {
                            Ok(response) => {
                                let message =
                                    format!("[Order] Binance Close Short Position Response: {:?}", response);
                                println!("{}", message);
                                log_event(&message).await;
                            }
                            Err(e) => eprintln!("[Order] Binance Close Short Position Failed: {}", e),
                        }
                    } else if binance_position == "LONG" {
                        match order.place_market_order_binance("XRPUSDT", "SELL", 100.0).await {
                            Ok(response) => {
                                let message =
                                    format!("[Order] Binance Close Long Position Response: {:?}", response);
                                println!("{}", message);
                                log_event(&message).await;
                            }
                            Err(e) => eprintln!("[Order] Binance Close Long Position Failed: {}", e),
                        }
                    }

                    if bitmart_position == "SHORT" {
                        match order.place_market_order_bitmart("XRPUSDT", "buy", 100.0).await {
                            Ok(response) => {
                                let message =
                                    format!("[Order] Bitmart Close Short Position Response: {:?}", response);
                                println!("{}", message);
                                log_event(&message).await;
                            }
                            Err(e) => eprintln!("[Order] Bitmart Close Short Position Failed: {}", e),
                        }
                    } else if bitmart_position == "LONG" {
                        match order.place_market_order_bitmart("XRPUSDT", "sell", 100.0).await {
                            Ok(response) => {
                                let message =
                                    format!("[Order] Bitmart Close Long Position Response: {:?}", response);
                                println!("{}", message);
                                log_event(&message).await;
                            }
                            Err(e) => eprintln!("[Order] Bitmart Close Long Position Failed: {}", e),
                        }
                    }
                }

                state.is_trading = false;
                state.entry_gap = None;
                state.binance_position = None;
                state.bitmart_position = None;
            }
        }
    }
}
