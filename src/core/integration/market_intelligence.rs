use anyhow::Result;
use super::UnifiedMarketData;

#[derive(Debug, Clone)]
pub struct MarketSignal {
    pub signal_type: SignalType,
    pub strength: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignalType {
    StrongBuy,
    Buy,
    Neutral,
    Sell,
    StrongSell,
}

pub struct MarketIntelligence {}

impl MarketIntelligence {
    pub fn new() -> Self {
        Self {}
    }

    pub fn analyze(&self, data: &UnifiedMarketData) -> Result<Vec<MarketSignal>> {
        let mut signals = Vec::new();

        // Volume analysis
        signals.push(self.analyze_volume(data));

        // Spread analysis
        signals.push(self.analyze_spread(data));

        // Order book imbalance
        signals.push(self.analyze_order_book(data));

        // NEW listing signal
        if data.is_new_listing {
            signals.push(MarketSignal {
                signal_type: SignalType::Buy,
                strength: 0.7,
                reason: "🆕 NEW LISTING - High opportunity".to_string(),
            });
        }

        // Liquidity signal
        signals.push(self.analyze_liquidity(data));

        Ok(signals)
    }

    pub fn get_overall_signal(&self, signals: &[MarketSignal]) -> SignalType {
        if signals.is_empty() {
            return SignalType::Neutral;
        }

        let buy_score: f64 = signals.iter()
            .filter(|s| matches!(s.signal_type, SignalType::Buy | SignalType::StrongBuy))
            .map(|s| s.strength)
            .sum();

        let sell_score: f64 = signals.iter()
            .filter(|s| matches!(s.signal_type, SignalType::Sell | SignalType::StrongSell))
            .map(|s| s.strength)
            .sum();

        let net_score = buy_score - sell_score;

        if net_score > 1.5 {
            SignalType::StrongBuy
        } else if net_score > 0.5 {
            SignalType::Buy
        } else if net_score < -1.5 {
            SignalType::StrongSell
        } else if net_score < -0.5 {
            SignalType::Sell
        } else {
            SignalType::Neutral
        }
    }

    fn analyze_volume(&self, data: &UnifiedMarketData) -> MarketSignal {
        if data.volume_24h > 1_000_000.0 {
            MarketSignal {
                signal_type: SignalType::Buy,
                strength: 0.8,
                reason: format!("High volume: ${:.0}", data.volume_24h),
            }
        } else if data.volume_24h > 100_000.0 {
            MarketSignal {
                signal_type: SignalType::Neutral,
                strength: 0.5,
                reason: format!("Moderate volume: ${:.0}", data.volume_24h),
            }
        } else {
            MarketSignal {
                signal_type: SignalType::Sell,
                strength: 0.3,
                reason: "Low volume".to_string(),
            }
        }
    }

    fn analyze_spread(&self, data: &UnifiedMarketData) -> MarketSignal {
        let spread_bps = data.spread_bps();

        if spread_bps < 10.0 {
            MarketSignal {
                signal_type: SignalType::Buy,
                strength: 0.6,
                reason: format!("Tight spread: {:.1} bps", spread_bps),
            }
        } else if spread_bps < 50.0 {
            MarketSignal {
                signal_type: SignalType::Neutral,
                strength: 0.4,
                reason: format!("Acceptable spread: {:.1} bps", spread_bps),
            }
        } else {
            MarketSignal {
                signal_type: SignalType::Sell,
                strength: 0.6,
                reason: format!("Wide spread: {:.1} bps", spread_bps),
            }
        }
    }

    fn analyze_order_book(&self, data: &UnifiedMarketData) -> MarketSignal {
        let imbalance = data.order_book_imbalance;

        if imbalance > 0.3 {
            MarketSignal {
                signal_type: SignalType::Buy,
                strength: 0.7,
                reason: "Strong buy pressure".to_string(),
            }
        } else if imbalance < -0.3 {
            MarketSignal {
                signal_type: SignalType::Sell,
                strength: 0.7,
                reason: "Strong sell pressure".to_string(),
            }
        } else {
            MarketSignal {
                signal_type: SignalType::Neutral,
                strength: 0.3,
                reason: "Balanced order book".to_string(),
            }
        }
    }

    fn analyze_liquidity(&self, data: &UnifiedMarketData) -> MarketSignal {
        if data.liquidity_score > 0.8 {
            MarketSignal {
                signal_type: SignalType::Buy,
                strength: 0.5,
                reason: "Excellent liquidity".to_string(),
            }
        } else if data.liquidity_score > 0.5 {
            MarketSignal {
                signal_type: SignalType::Neutral,
                strength: 0.3,
                reason: "Adequate liquidity".to_string(),
            }
        } else {
            MarketSignal {
                signal_type: SignalType::Sell,
                strength: 0.4,
                reason: "Low liquidity".to_string(),
            }
        }
    }
}

