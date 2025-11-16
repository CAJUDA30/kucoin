//! Stop loss module placeholder.

pub fn compute_stop(entry_price: f64, pct: f64) -> f64 {
    entry_price * (1.0 - pct)
}
