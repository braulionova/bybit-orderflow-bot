use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::bybit::auth::BybitAuth;
use crate::bybit::types::*;

pub struct BybitClient {
    client: Client,
    auth: Option<BybitAuth>,
    rest_url: String,
    recv_window: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub qty: f64,
    pub price: Option<f64>,
    pub reduce_only: bool,
    pub close_on_trigger: bool,
    // Native Bybit SL/TP fields
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub tpsl_mode: Option<String>,           // "Full" or "Partial"
    pub tp_order_type: Option<String>,       // "Market" or "Limit"
    pub sl_order_type: Option<String>,       // "Market" or "Limit"
    pub tp_trigger_by: Option<String>,       // "LastPrice", "MarkPrice", "IndexPrice"
    pub sl_trigger_by: Option<String>,       // "LastPrice", "MarkPrice", "IndexPrice"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: f64,
    pub qty: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: String,
    pub size: f64,
    pub avg_price: f64,
    pub unrealised_pnl: f64,
    pub leverage: f64,
    pub liq_price: Option<f64>,
    pub margin: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub total_available_balance: f64,
    pub total_margin_balance: f64,
    pub total_perpetual_unrealised_pnl: f64,
}

impl BybitClient {
    pub fn new(rest_url: String, auth: Option<BybitAuth>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            auth,
            rest_url,
            recv_window: 5000,
        }
    }

    pub async fn place_order(&self, request: OrderRequest) -> Result<OrderResponse> {
        let url = format!("{}/v5/order/create", self.rest_url);
        
        let side_str = match request.side {
            OrderSide::Buy => "Buy",
            OrderSide::Sell => "Sell",
        };
        
        let order_type_str = match request.order_type {
            OrderType::Market => "Market",
            OrderType::Limit => "Limit",
        };
        
        let mut body = json!({
            "category": "linear",
            "symbol": request.symbol,
            "side": side_str,
            "orderType": order_type_str,
            "qty": format!("{:.3}", request.qty),
            "reduceOnly": request.reduce_only,
            "closeOnTrigger": request.close_on_trigger,
        });

        if let Some(price) = request.price {
            body["price"] = json!(price.to_string());
        }

        // Add native SL/TP if provided
        if let Some(sl) = request.stop_loss {
            body["stopLoss"] = json!(format!("{:.2}", sl));
            body["slOrderType"] = json!(request.sl_order_type.as_deref().unwrap_or("Market"));
            body["slTriggerBy"] = json!(request.sl_trigger_by.as_deref().unwrap_or("LastPrice"));
        }
        if let Some(tp) = request.take_profit {
            body["takeProfit"] = json!(format!("{:.2}", tp));
            body["tpOrderType"] = json!(request.tp_order_type.as_deref().unwrap_or("Market"));
            body["tpTriggerBy"] = json!(request.tp_trigger_by.as_deref().unwrap_or("LastPrice"));
        }
        if request.stop_loss.is_some() || request.take_profit.is_some() {
            body["tpslMode"] = json!(request.tpsl_mode.as_deref().unwrap_or("Full"));
        }

        let response = self_signed_post(&self.client, &url, &self.auth, body.clone(), self.recv_window).await?;
        
        let resp_json: serde_json::Value = response.json().await?;
        
        let ret_code = resp_json["retCode"].as_i64().unwrap_or(-1);
        if ret_code != 0 {
            let error_msg = resp_json["retMsg"].as_str().unwrap_or("Unknown error");
            let error_detail = resp_json.get("result").map(|r| r.to_string()).unwrap_or_default();

            // Log warning if SL/TP was rejected
            if error_msg.contains("stopLoss") || error_msg.contains("takeProfit") ||
               error_msg.contains("slPrice") || error_msg.contains("tpPrice") {
                tracing::warn!("⚠️  Bybit rejected SL/TP - position will rely on software monitoring");
            }

            anyhow::bail!("Order failed: {} | Detail: {} | Request: {}", error_msg, error_detail, body);
        }

        let order_data = &resp_json["result"]["orderId"];
        
        Ok(OrderResponse {
            order_id: order_data.as_str().unwrap_or("").to_string(),
            symbol: request.symbol,
            side: format!("{:?}", request.side),
            order_type: format!("{:?}", request.order_type),
            price: request.price.unwrap_or(0.0),
            qty: request.qty,
            status: "Created".to_string(),
        })
    }

    pub async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()> {
        let url = format!("{}/v5/order/cancel", self.rest_url);
        
        let body = json!({
            "category": "linear",
            "symbol": symbol,
            "orderId": order_id,
        });

        let response = self_signed_post(&self.client, &url, &self.auth, body, self.recv_window).await?;
        
        let resp_json: serde_json::Value = response.json().await?;
        
        let ret_code = resp_json["retCode"].as_i64().unwrap_or(-1);
        if ret_code != 0 {
            anyhow::bail!("Cancel failed: {:?}", resp_json["retMsg"]);
        }

        Ok(())
    }

    pub async fn get_positions(&self, symbol: Option<&str>) -> Result<Vec<Position>> {
        let url = format!("{}/v5/position/closed-pnl", self.rest_url);
        
        let mut body = json!({
            "category": "linear",
        });

        if let Some(s) = symbol {
            body["symbol"] = json!(s);
        }

        let response = self_signed_get(&self.client, &url, &self.auth, body, self.recv_window).await?;
        
        let resp_json: serde_json::Value = response.json().await?;
        
        let ret_code = resp_json["retCode"].as_i64().unwrap_or(-1);
        if ret_code != 0 {
            anyhow::bail!("Get positions failed: {:?}", resp_json["retMsg"]);
        }

        let positions: Vec<Position> = serde_json::from_value(
            resp_json["result"]["list"].clone()
        ).unwrap_or_default();

        Ok(positions)
    }

    pub async fn get_wallet(&self) -> Result<Wallet> {
        let url = format!("{}/v5/account/wallet-balance", self.rest_url);
        
        let body = json!({
            "accountType": "UNIFIED",
        });

        let response = self_signed_get(&self.client, &url, &self.auth, body, self.recv_window).await?;
        
        let resp_json: serde_json::Value = response.json().await?;
        
        let ret_code = resp_json["retCode"].as_i64().unwrap_or(-1);
        if ret_code != 0 {
            anyhow::bail!("Get wallet failed: {:?}", resp_json["retMsg"]);
        }

        let list = &resp_json["result"]["list"][0];
        
        Ok(Wallet {
            total_available_balance: list["totalAvailableBalance"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            total_margin_balance: list["totalMarginBalance"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
            total_perpetual_unrealised_pnl: list["totalPerpetualUnrealisedPnl"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0),
        })
    }

    pub async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<()> {
        let url = format!("{}/v5/position/set-leverage", self.rest_url);
        
        let body = json!({
            "category": "linear",
            "symbol": symbol,
            "buyLeverage": leverage,
            "sellLeverage": leverage,
        });

        let response = self_signed_post(&self.client, &url, &self.auth, body, self.recv_window).await?;
        
        let resp_json: serde_json::Value = response.json().await?;
        
        let ret_code = resp_json["retCode"].as_i64().unwrap_or(-1);
        if ret_code != 0 {
            anyhow::bail!("Set leverage failed: {:?}", resp_json["retMsg"]);
        }

        Ok(())
    }
}

async fn self_signed_post(
    client: &Client,
    url: &str,
    auth: &Option<BybitAuth>,
    body: serde_json::Value,
    recv_window: u64,
) -> Result<reqwest::Response> {
    match auth {
        Some(auth) => {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let body_str = body.to_string();
            
            let sign = auth.generate_signature(timestamp as u64, &format!("{}{}", recv_window, &body_str));
            
            let response = client
                .post(url)
                .header("X-BAPI-API-KEY", auth.get_api_key())
                .header("X-BAPI-SIGN", sign)
                .header("X-BAPI-SIGN-TYPE", "2")
                .header("X-BAPI-TIMESTAMP", timestamp.to_string())
                .header("X-BAPI-RECV-WINDOW", recv_window.to_string())
                .header("Content-Type", "application/json")
                .body(body_str)
                .send()
                .await
                .context("HTTP request failed")?;
            
            Ok(response)
        }
        None => {
            let response = client
                .post(url)
                .header("Content-Type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .context("HTTP request failed")?;
            
            Ok(response)
        }
    }
}

async fn self_signed_get(
    client: &Client,
    url: &str,
    auth: &Option<BybitAuth>,
    params: serde_json::Value,
    recv_window: u64,
) -> Result<reqwest::Response> {
    match auth {
        Some(auth) => {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let query_string = serde_json::to_string(&params)?;
            
            let sign = auth.generate_signature(timestamp as u64, &format!("{}{}", recv_window, &query_string));
            
            let response = client
                .get(url)
                .query(&serde_json::from_value::<serde_json::Value>(params)?)
                .header("X-BAPI-API-KEY", auth.get_api_key())
                .header("X-BAPI-SIGN", sign)
                .header("X-BAPI-SIGN-TYPE", "2")
                .header("X-BAPI-TIMESTAMP", timestamp.to_string())
                .header("X-BAPI-RECV-WINDOW", recv_window.to_string())
                .send()
                .await
                .context("HTTP request failed")?;
            
            Ok(response)
        }
        None => {
            let response = client
                .get(url)
                .query(&serde_json::from_value::<serde_json::Value>(params)?)
                .send()
                .await
                .context("HTTP request failed")?;
            
            Ok(response)
        }
    }
}
