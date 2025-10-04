use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketData {
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct FeedHandler {
    url: String,
    data: Arc<RwLock<Vec<MarketData>>>,
}

impl FeedHandler {
    pub fn new(url: String) -> Self {
        Self {
            url,
            data: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to WebSocket: {}", self.url);

        let (ws_stream, _) = connect_async(&self.url).await?;
        info!("WebSocket connected successfully");

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to market data
        let subscribe_msg = serde_json::json!({
            "type": "subscribe",
            "channels": ["ticker", "trades"]
        });

        write
            .send(Message::Text(subscribe_msg.to_string()))
            .await?;

        info!("Subscribed to market data channels");

        // Process incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(market_data) = serde_json::from_str::<MarketData>(&text) {
                        info!(
                            "Received: {} @ ${:.2} (vol: {:.2})",
                            market_data.symbol, market_data.price, market_data.volume
                        );

                        let mut data = self.data.write().await;
                        data.push(market_data);

                        // Keep only last 1000 records
                        if data.len() > 1000 {
                            data.remove(0);
                        }
                    }
                }
                Ok(Message::Ping(payload)) => {
                    write.send(Message::Pong(payload)).await?;
                }
                Ok(Message::Close(_)) => {
                    warn!("WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn get_latest_data(&self) -> Vec<MarketData> {
        let data = self.data.read().await;
        data.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("=== Rust WebSocket Feed Handler ===");

    // Example: Connect to a mock WebSocket endpoint
    // In production, replace with actual exchange WebSocket URL
    let feed_url = "wss://stream.example.com/ws";

    let handler = FeedHandler::new(feed_url.to_string());

    // Spawn connection task
    let handler_clone = handler.clone();
    tokio::spawn(async move {
        if let Err(e) = handler_clone.connect().await {
            error!("Connection error: {}", e);
        }
    });

    // Simulate reading data periodically
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
    loop {
        interval.tick().await;

        let data = handler.get_latest_data().await;
        info!("Total records in memory: {}", data.len());

        if !data.is_empty() {
            if let Some(latest) = data.last() {
                info!("Latest: {} @ ${:.2}", latest.symbol, latest.price);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_feed_handler_creation() {
        let handler = FeedHandler::new("wss://test.com".to_string());
        assert_eq!(handler.url, "wss://test.com");
    }

    #[tokio::test]
    async fn test_data_storage() {
        let handler = FeedHandler::new("wss://test.com".to_string());

        let market_data = MarketData {
            symbol: "BTCUSD".to_string(),
            price: 50000.0,
            volume: 1.5,
            timestamp: 1234567890,
        };

        {
            let mut data = handler.data.write().await;
            data.push(market_data.clone());
        }

        let stored_data = handler.get_latest_data().await;
        assert_eq!(stored_data.len(), 1);
        assert_eq!(stored_data[0].symbol, "BTCUSD");
    }
}
