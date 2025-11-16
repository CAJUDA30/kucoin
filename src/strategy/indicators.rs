use std::collections::VecDeque;

pub struct Indicators {
    prices: VecDeque<f64>,
    volumes: VecDeque<f64>,
    max_len: usize,
}

impl Indicators {
    pub fn new(max_len: usize) -> Self {
        Self {
            prices: VecDeque::with_capacity(max_len),
            volumes: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    pub fn add_data(&mut self, price: f64, volume: f64) {
        if self.prices.len() >= self.max_len {
            self.prices.pop_front();
            self.volumes.pop_front();
        }

        self.prices.push_back(price);
        self.volumes.push_back(volume);
    }

    pub fn sma(&self, period: usize) -> Option<f64> {
        if self.prices.len() < period {
            return None;
        }

        let sum: f64 = self.prices.iter().rev().take(period).sum();
        Some(sum / period as f64)
    }

    pub fn ema(&self, period: usize) -> Option<f64> {
        if self.prices.len() < period {
            return None;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = self.prices[0];

        for price in self.prices.iter().skip(1) {
            ema = (price - ema) * multiplier + ema;
        }

        Some(ema)
    }

    pub fn rsi(&self, period: usize) -> Option<f64> {
        if self.prices.len() < period + 1 {
            return None;
        }

        let mut gains = 0.0;
        let mut losses = 0.0;

        for i in (self.prices.len() - period)..self.prices.len() {
            let change = self.prices[i] - self.prices[i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses += -change;
            }
        }

        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;

        if avg_loss == 0.0 {
            return Some(100.0);
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    pub fn bollinger_bands(&self, period: usize, std_dev: f64) -> Option<(f64, f64, f64)> {
        let sma = self.sma(period)?;

        if self.prices.len() < period {
            return None;
        }

        let recent_prices: Vec<f64> = self.prices.iter().rev().take(period).copied().collect();
        let variance: f64 = recent_prices
            .iter()
            .map(|p| (p - sma).powi(2))
            .sum::<f64>()
            / period as f64;

        let std = variance.sqrt();
        let upper = sma + (std_dev * std);
        let lower = sma - (std_dev * std);

        Some((upper, sma, lower))
    }

    pub fn volatility(&self, period: usize) -> Option<f64> {
        if self.prices.len() < period {
            return None;
        }

        let recent: Vec<f64> = self.prices.iter().rev().take(period).copied().collect();
        let mean = recent.iter().sum::<f64>() / period as f64;
        let variance = recent
            .iter()
            .map(|p| (p - mean).powi(2))
            .sum::<f64>()
            / period as f64;

        Some(variance.sqrt() / mean)
    }

    pub fn volume_ratio(&self, period: usize) -> Option<f64> {
        if self.volumes.len() < period {
            return None;
        }

        let recent_avg: f64 =
            self.volumes.iter().rev().take(period).sum::<f64>() / period as f64;
        let latest = *self.volumes.back()?;

        Some(latest / recent_avg)
    }
}

