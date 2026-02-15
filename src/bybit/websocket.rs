use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use futures::{SinkExt, StreamExt};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{info, warn, error, debug};
use anyhow::Result;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub type MessageHandler = Arc<dyn Fn(serde_json::Value) -> Result<()> + Send + Sync>;

pub struct BybitWebSocket {
    url: String,
    subscriptions: Vec<String>,
    handlers: Arc<DashMap<String, MessageHandler>>,
}

impl BybitWebSocket {
    pub fn new(url: String) -> Self {
        Self {
            url,
            subscriptions: Vec::new(),
            handlers: Arc::new(DashMap::new()),
        }
    }
    
    pub fn subscribe(&mut self, topic: String, handler: MessageHandler) {
        info!("Subscribing to topic: {}", topic);
        self.subscriptions.push(topic.clone());
        self.handlers.insert(topic, handler);
    }
    
    pub async fn connect(&self) -> Result<WsStream> {
        info!("Connecting to Bybit WebSocket: {}", self.url);
        
        let (ws_stream, response) = connect_async(&self.url).await?;
        
        info!("WebSocket connected: {:?}", response.status());
        
        Ok(ws_stream)
    }
    
    pub async fn run(self: Arc<Self>) -> Result<()> {
        loop {
            match self.connect().await {
                Ok(ws_stream) => {
                    if let Err(e) = self.handle_stream(ws_stream).await {
                        error!("WebSocket error: {}", e);
                        warn!("Reconnecting in 5s...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
    
    async fn handle_stream(&self, ws_stream: WsStream) -> Result<()> {
        let (mut write, mut read) = ws_stream.split();
        
        // Send subscriptions
        for topic in &self.subscriptions {
            let sub_msg = serde_json::json!({
                "op": "subscribe",
                "args": [topic]
            });
            
            write.send(Message::Text(sub_msg.to_string())).await?;
            info!("Subscribed to: {}", topic);
        }
        
        // Ping task (keep-alive every 20s)
        let ping_task = {
            let write = Arc::new(tokio::sync::Mutex::new(write));
            let write_clone = write.clone();
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(
                    tokio::time::Duration::from_secs(20)
                );
                
                loop {
                    interval.tick().await;
                    let mut w = write_clone.lock().await;
                    if w.send(Message::Ping(vec![])).await.is_err() {
                        break;
                    }
                }
            })
        };
        
        // Message processing loop
        while let Some(msg) = read.next().await {
            let msg = msg?;
            
            match msg {
                Message::Text(text) => {
                    let start = std::time::Instant::now();
                    
                    // Parse JSON
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(parsed) => {
                            let parse_latency = start.elapsed().as_micros();
                            if parse_latency > 100 {
                                warn!("Slow JSON parse: {}μs", parse_latency);
                            }
                            
                            // Route to handler
                            self.route_message(parsed).await;
                        }
                        Err(e) => {
                            error!("Failed to parse message: {}", e);
                        }
                    }
                }
                Message::Pong(_) => {
                    debug!("Pong received");
                }
                Message::Close(_) => {
                    warn!("WebSocket closed by server");
                    break;
                }
                _ => {}
            }
        }
        
        ping_task.abort();
        Ok(())
    }
    
    async fn route_message(&self, msg: serde_json::Value) {
        // Extract topic from message
        if let Some(topic_value) = msg.get("topic") {
            if let Some(topic_str) = topic_value.as_str() {
                // Find matching handler by prefix
                for handler_entry in self.handlers.iter() {
                    let topic_key = handler_entry.key();
                    
                    // Match exact or prefix (e.g., "orderbook.50.BTCUSDT" matches "orderbook.50.")
                    if topic_str == topic_key || topic_str.starts_with(topic_key) {
                        if let Some(data) = msg.get("data") {
                            let handler = handler_entry.value().clone();
                            let data_clone = data.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = handler(data_clone) {
                                    error!("Handler error: {}", e);
                                }
                            });
                        }
                        break;
                    }
                }
            }
        }
    }
}
