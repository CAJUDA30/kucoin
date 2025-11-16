use anyhow::Result;

use super::indicators::Indicators;
use super::signals::{SignalType, TradeSignal};
use crate::scanner::market_scanner::MarketSnapshot;

pub struct AIStrategyEngine {
    indicators: Indicators,
}

impl AIStrategyEngine {
    pub fn new() -> Self {
        Self {
            indicators: Indicators::new(200), // 200 data points
        }
    }

    pub fn analyze(&mut self, snapshot: &MarketSnapshot) -> Result<Option<TradeSignal>> {
        // Update indicators
        self.indicators
            .add_data(snapshot.price, snapshot.volume_24h);

        // Multi-factor analysis
        let technical_score = self.technical_analysis()?;
        let volume_score = self.volume_analysis()?;
        let volatility_score = self.volatility_analysis()?;

        // Ensemble scoring
        let total_score =
            (technical_score * 0.5) + (volume_score * 0.3) + (volatility_score * 0.2);

        tracing::debug!(
            "Analysis for {}: tech={:.2}, vol={:.2}, vola={:.2}, total={:.2}",
            snapshot.symbol,
            technical_score,
            volume_score,
            volatility_score,
            total_score
        );

        // Generate signal if score is strong enough
        if total_score > 0.7 {
            Ok(Some(
                self.generate_signal(snapshot, total_score, SignalType::Long)?,
            ))
        } else if total_score < -0.7 {
            Ok(Some(
                self.generate_signal(snapshot, total_score.abs(), SignalType::Short)?,
            ))
        } else {
            Ok(None)
        }
    }

    fn technical_analysis(&self) -> Result<f64> {
        let mut score: f64 = 0.0;

        // RSI analysis
        if let Some(rsi) = self.indicators.rsi(14) {
            if rsi < 30.0 {
                score += 0.3; // Oversold (bullish)
            } else if rsi > 70.0 {
                score -= 0.3; // Overbought (bearish)
            }
        }

        // Moving average crossover
        if let (Some(sma_fast), Some(sma_slow)) =
            (self.indicators.sma(10), self.indicators.sma(50))
        {
            if sma_fast > sma_slow * 1.02 {
                score += 0.4; // Bullish crossover
            } else if sma_fast < sma_slow * 0.98 {
                score -= 0.4; // Bearish crossover
            }
        }

        // Bollinger bands
        if let Some((upper, _middle, lower)) = self.indicators.bollinger_bands(20, 2.0) {
            if let Some(current) = self.indicators.sma(1) {
                if current < lower {
                    score += 0.3; // Below lower band (bullish)
                } else if current > upper {
                    score -= 0.3; // Above upper band (bearish)
                }
            }
        }

        Ok(score.max(-1.0_f64).min(1.0_f64))
    }

    fn volume_analysis(&self) -> Result<f64> {
        let mut score: f64 = 0.0;

        if let Some(volume_ratio) = self.indicators.volume_ratio(20) {
            if volume_ratio > 2.0 {
                score = 0.5; // High volume confirms trend
            } else if volume_ratio > 1.5 {
                score = 0.3;
            } else if volume_ratio < 0.5 {
                score = -0.3; // Low volume is bearish
            }
        }

        Ok(score)
    }

    fn volatility_analysis(&self) -> Result<f64> {
        let mut score: f64 = 0.0;

        if let Some(volatility) = self.indicators.volatility(20) {
            if volatility > 0.05 {
                score = 0.3; // High volatility = opportunity
            } else if volatility > 0.03 {
                score = 0.2;
            } else if volatility < 0.01 {
                score = -0.3; // Low volatility = avoid
            }
        }

        Ok(score)
    }

    fn generate_signal(
        &self,
        snapshot: &MarketSnapshot,
        confidence: f64,
        signal_type: SignalType,
    ) -> Result<TradeSignal> {
        let signal = TradeSignal::new(
            snapshot.symbol.clone(),
            signal_type,
            confidence,
            snapshot.price,
        );

        let reason = format!(
            "AI Score: {:.2} | Vol: {:.0} | Volatility: {:.3}",
            confidence, snapshot.volume_24h, snapshot.volatility
        );

        Ok(signal.with_reason(reason))
    }
}

