use bybit_orderflow_bot::orderbook::Orderbook;

#[test]
fn test_orderbook_operations() {
    let ob = Orderbook::new("BTCUSDT".to_string());
    
    // Test snapshot
    let bids = vec![
        (50000.0, 1.5),
        (49999.0, 2.0),
        (49998.0, 1.2),
    ];
    
    let asks = vec![
        (50001.0, 1.3),
        (50002.0, 1.8),
        (50003.0, 2.1),
    ];
    
    ob.apply_snapshot(bids, asks);
    
    // Verify best prices
    let (bid, ask) = ob.best_bid_ask();
    assert_eq!(bid, 50000.0);
    assert_eq!(ask, 50001.0);
    
    // Verify mid price
    let mid = ob.mid_price();
    assert_eq!(mid, 50000.5);
    
    // Verify spread
    let spread = ob.spread_pct();
    assert!(spread > 0.0);
    assert!(spread < 0.01);
    
    // Test imbalance
    let imbalance = ob.imbalance(3);
    assert!(imbalance > 0.0); // More bid volume
    
    // Test liquidity
    let liquidity = ob.liquidity_depth(3);
    assert!(liquidity > 0.0);
    
    println!("✅ All orderbook tests passed!");
}

#[test]
fn test_orderbook_delta_updates() {
    let ob = Orderbook::new("BTCUSDT".to_string());
    
    // Initial snapshot
    let bids = vec![(50000.0, 1.0)];
    let asks = vec![(50001.0, 1.0)];
    ob.apply_snapshot(bids, asks);
    
    // Apply delta - update bid
    let delta_bids = vec![(50000.5, 2.0)];
    let delta_asks = vec![];
    ob.apply_delta(delta_bids, delta_asks);
    
    let (bid, _) = ob.best_bid_ask();
    assert_eq!(bid, 50000.5);
    
    // Apply delta - remove level
    let delta_bids = vec![(50000.5, 0.0)];
    let delta_asks = vec![];
    ob.apply_delta(delta_bids, delta_asks);
    
    let (bid, _) = ob.best_bid_ask();
    assert_eq!(bid, 50000.0);
    
    println!("✅ Delta update tests passed!");
}
