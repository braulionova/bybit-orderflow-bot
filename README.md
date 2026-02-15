# Bybit Order Flow Bot - Phase 1

Ultra-low latency trading bot for Bybit order flow analysis.

## Phase 1 Implementation ✅

This is the **Phase 1** implementation including:

- ✅ Rust project structure with optimized build configuration
- ✅ Configuration management (TOML + environment variables)
- ✅ Bybit WebSocket client with auto-reconnect
- ✅ HMAC authentication for Bybit API
- ✅ Lock-free orderbook manager using `dashmap` and atomic operations
- ✅ Real-time orderbook monitoring and metrics
- ✅ Latency tracking and alerts

## Quick Start

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env with your API credentials (optional for public data)
```

### 3. Build and Run

```bash
# Development build
cargo build

# Run the bot
cargo run

# Optimized release build
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Run release build
./target/release/bybit-orderflow-bot
```

### 4. Run Tests

```bash
cargo test
```

## Configuration

Edit `config/default.toml` or use environment variables:

```bash
export BOT_TRADING_SYMBOL=BTCUSDT
export BOT_BYBIT_TESTNET=true
export RUST_LOG=info
```

## Performance Targets (Phase 1)

- WebSocket message processing: <100μs
- Orderbook delta updates: <50μs
- Imbalance calculations: <15μs
- Memory usage: <100MB

## Architecture

```
src/
├── config/          Configuration management
├── bybit/          
│   ├── types.rs     Bybit data structures
│   ├── auth.rs      HMAC authentication
│   └── websocket.rs WebSocket client
├── orderbook/       
│   └── manager.rs   Lock-free orderbook
└── main.rs          Application entry point
```

## Features

### Implemented ✅

- Real-time orderbook tracking
- Lock-free concurrent data structures
- Automatic WebSocket reconnection
- Latency monitoring
- Spread and liquidity alerts
- Imbalance calculation

### Coming in Phase 2

- Market data aggregation (trades, liquidations)
- Bias engine (market regime detection)
- Signal scoring system
- Risk management (kill switch, drawdown monitor)
- Execution engine
- Backtesting framework

## Monitoring

The bot logs key metrics every 5 seconds:

```
📊 BTCUSDT | Bid: $50000.00 | Ask: $50001.00 | Mid: $50000.50 | 
   Spread: 0.0020% | Imbalance: 0.150 | Liquidity: 12.50 BTC | 
   Latency: 45ms | Updates: 1234
```

Alerts:
- ⚠️  Wide spread (>0.8%)
- ⚠️  High latency (>150ms)
- ⚠️  Low liquidity (<5 BTC)

## Next Steps

1. Test connection to Bybit testnet
2. Monitor orderbook metrics
3. Verify latency targets
4. Prepare for Phase 2 (strategy implementation)

## License

MIT
