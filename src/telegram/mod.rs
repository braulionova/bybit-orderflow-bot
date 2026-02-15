use anyhow::Result;
use reqwest::Client;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;

const STARTUP_COOLDOWN_SECS: u64 = 600; // 10 minutes
const LAST_STARTUP_FILE: &str = "/tmp/bybit-orderflow-bot/last_startup.txt";

fn get_last_startup_time() -> u64 {
    if let Ok(content) = fs::read_to_string(LAST_STARTUP_FILE) {
        content.trim().parse().unwrap_or(0)
    } else {
        0
    }
}

fn set_last_startup_time(time: u64) {
    let _ = fs::write(LAST_STARTUP_FILE, time.to_string());
}

pub struct TelegramNotifier {
    client: Client,
    bot_token: String,
    chat_id: String,
    last_startup: Arc<AtomicU64>,
}

impl Clone for TelegramNotifier {
    fn clone(&self) -> Self {
        Self {
            client: Client::new(),
            bot_token: self.bot_token.clone(),
            chat_id: self.chat_id.clone(),
            last_startup: Arc::clone(&self.last_startup),
        }
    }
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            client: Client::new(),
            bot_token,
            chat_id,
            last_startup: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn send_message(&self, message: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        self.client
            .post(&url)
            .json(&json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": "HTML"
            }))
            .send()
            .await?;

        Ok(())
    }

    pub async fn notify_startup(&self, symbol: &str, testnet: bool) -> Result<bool> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let last = get_last_startup_time();
        
        if now - last < STARTUP_COOLDOWN_SECS {
            eprintln!("[TG] Startup notification blocked, cooldown active. Last: {}, Now: {}, Diff: {}", last, now, now - last);
            return Ok(false);
        }

        eprintln!("[TG] Sending startup notification. Last: {}, Now: {}", last, now);
        set_last_startup_time(now);

        let network = if testnet { "TESTNET" } else { "MAINNET" };
        let message = format!(
            "🤖 <b>Bot Started</b>\n\n\
             📊 Symbol: {}\n\
             🌐 Network: {}\n\
             ✅ Status: Running",
            symbol, network
        );
        self.send_message(&message).await?;
        Ok(true)
    }

    pub async fn notify_shutdown(&self, symbol: &str) -> Result<()> {
        let message = format!(
            "🛑 <b>Bot Stopped</b>\n\n\
             📊 Symbol: {}",
            symbol
        );
        self.send_message(&message).await
    }

    pub async fn notify_error(&self, symbol: &str, error: &str) -> Result<()> {
        let message = format!(
            "⚠️ <b>Error</b>\n\n\
             📊 Symbol: {}\n\
             ❌ Error: {}",
            symbol, error
        );
        self.send_message(&message).await
    }

    pub async fn notify_summary(&self, symbol: &str, bid: f64, ask: f64, spread: f64, imbalance: f64, liquidity: f64, latency: u64, updates: u64) -> Result<()> {
        let message = format!(
            "📊 <b>Resumen 5 min</b>\n\n\
             📊 Symbol: {}\n\
             💰 Bid: ${:.2}\n\
             💵 Ask: ${:.2}\n\
             📈 Spread: {:.4}%\n\
             ⚖️ Imbalance: {:.3}\n\
             💧 Liquidity: {:.2} BTC\n\
             ⏱️ Latency: {}ms\n\
             🔄 Updates: {}",
            symbol, bid, ask, spread, imbalance, liquidity, latency, updates
        );
        self.send_message(&message).await
    }

    pub async fn notify_order_placed(&self, symbol: &str, side: &str, order_type: &str, qty: f64, price: f64, order_id: &str) -> Result<()> {
        let emoji = if side == "Buy" { "🟢" } else { "🔴" };
        let message = format!(
            "📤 <b>Orden Colocada</b>\n\n\
             {} Side: <b>{}</b>\n\
             📊 Symbol: {}\n\
             📝 Type: {}\n\
             🔢 Qty: {:.4}\n\
             💵 Price: ${:.2}\n\
             🆔 Order ID: {}",
            emoji, side, symbol, order_type, qty, price, order_id
        );
        self.send_message(&message).await
    }

    pub async fn notify_order_error(&self, symbol: &str, side: &str, error: &str) -> Result<()> {
        let emoji = if side == "Buy" { "🟢" } else { "🔴" };
        let message = format!(
            "❌ <b>Orden Fallida</b>\n\n\
             {} Side: <b>{}</b>\n\
             📊 Symbol: {}\n\
             ⚠️ Error: {}",
            emoji, side, symbol, error
        );
        self.send_message(&message).await
    }

    pub async fn notify_position_closed(&self, symbol: &str, side: &str, entry_price: f64, exit_price: f64, qty: f64, pnl: f64, pnl_pct: f64, reason: &str) -> Result<()> {
        let (emoji, pnl_emoji) = if pnl >= 0.0 { ("💰", "🟢") } else { ("📉", "🔴") };
        let message = format!(
            "🔒 <b>Posición Cerrada</b>\n\n\
             📊 Symbol: {}\n\
             {} Side: <b>{}</b>\n\
             🚪 Entry: ${:.2}\n\
             🚪 Exit: ${:.2}\n\
             🔢 Qty: {:.4}\n\
             {}{} PnL: ${:.2} ({:.2}%)\n\
             📋 Reason: {}",
            symbol, pnl_emoji, side, entry_price, exit_price, qty, pnl_emoji, pnl_emoji, pnl, pnl_pct, reason
        );
        self.send_message(&message).await
    }

    pub async fn notify_position_opened(&self, symbol: &str, side: &str, entry_price: f64, qty: f64, stop_loss: f64, take_profit: f64) -> Result<()> {
        let emoji = if side == "Buy" { "🟢" } else { "🔴" };
        let message = format!(
            "📍 <b>Posición Abierta</b>\n\n\
             📊 Symbol: {}\n\
             {} Side: <b>{}</b>\n\
             🎯 Entry: ${:.2}\n\
             🔢 Qty: {:.4}\n\
             🛡️ Stop Loss: ${:.2}\n\
             🎯 Take Profit: ${:.2}",
            symbol, emoji, side, entry_price, qty, stop_loss, take_profit
        );
        self.send_message(&message).await
    }

    pub async fn notify_wallet(&self, balance: f64, available: f64, unrealized_pnl: f64) -> Result<()> {
        let emoji = if unrealized_pnl >= 0.0 { "🟢" } else { "🔴" };
        let message = format!(
            "💳 <b>Wallet Balance</b>\n\n\
             💰 Total: <b>${:.2}</b>\n\
             💵 Available: ${:.2}\n\
             {} Unrealized PnL: ${:.2}",
            balance, available, emoji, unrealized_pnl
        );
        self.send_message(&message).await
    }
}
