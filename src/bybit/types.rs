use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrderbookData {
    #[serde(rename = "s")]
    pub symbol: String,
    
    #[serde(rename = "b")]
    pub bids: Vec<(String, String)>,
    
    #[serde(rename = "a")]
    pub asks: Vec<(String, String)>,
    
    #[serde(rename = "u")]
    pub update_id: u64,
    
    #[serde(rename = "seq")]
    pub seq: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TradeData {
    #[serde(rename = "T")]
    pub timestamp: u64,
    
    #[serde(rename = "s")]
    pub symbol: String,
    
    #[serde(rename = "S")]
    pub side: String,
    
    #[serde(rename = "v")]
    pub size: String,
    
    #[serde(rename = "p")]
    pub price: String,
    
    #[serde(rename = "i")]
    pub trade_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LiquidationData {
    #[serde(rename = "symbol")]
    pub symbol: String,
    
    #[serde(rename = "side")]
    pub side: String,
    
    #[serde(rename = "price")]
    pub price: String,
    
    #[serde(rename = "size")]
    pub size: String,
    
    #[serde(rename = "updatedTime")]
    pub updated_time: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WsMessage {
    pub topic: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub data: Option<serde_json::Value>,
    pub ts: Option<u64>,
}

impl OrderbookData {
    pub fn parse_levels(&self) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
        let bids: Vec<(f64, f64)> = self.bids.iter()
            .filter_map(|(p, q)| {
                Some((p.parse().ok()?, q.parse().ok()?))
            })
            .collect();
        
        let asks: Vec<(f64, f64)> = self.asks.iter()
            .filter_map(|(p, q)| {
                Some((p.parse().ok()?, q.parse().ok()?))
            })
            .collect();
        
        (bids, asks)
    }
}
