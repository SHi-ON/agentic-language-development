//! Descriptive statistics, Wilson intervals, bootstrap, t-tests.
//!
//! Ports `packages/analysis/src/{descriptive,bootstrap,hypothesis}.ts`.

use crate::special::normal_quantile;
use crate::AnalysisError;
use ald_hashing::SeededPrng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
    pub level: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DescriptiveSummary {
    pub n: usize,
    pub mean: f64,
    pub variance: f64,
    pub sd: f64,
    pub min: f64,
    pub max: f64,
}

fn assert_sample(values: &[f64], name: &str) -> Result<(), AnalysisError> {
    if values.is_empty() {
        return Err(AnalysisError::EmptySample(format!("{name} must not be empty")));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err(AnalysisError::Domain(format!("{name} must be finite")));
    }
    Ok(())
}

fn assert_level(level: f64, name: &str) -> Result<(), AnalysisError> {
    if !(level > 0.0 && level < 1.0) {
        return Err(AnalysisError::Domain(format!("{name} must be in (0, 1)")));
    }
    Ok(())
}

/// Sample summary with unbiased (n-1) variance; variance/sd are NaN at n=1;
/// exact 0.0 variance for point masses (matches TS).
pub fn summarize(values: &[f64]) -> Result<DescriptiveSummary, AnalysisError> {
    assert_sample(values, "values")?;
    let n = values.len();
    let mut total = 0.0;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in values {
        total += v;
        if v < min {
            min = v;
        }
        if v > max {
            max = v;
        }
    }
    let mean = total / n as f64;
    if n < 2 {
        return Ok(DescriptiveSummary { n, mean, variance: f64::NAN, sd: f64::NAN, min, max });
    }
    if min == max {
        return Ok(DescriptiveSummary { n, mean: min, variance: 0.0, sd: 0.0, min, max });
    }
    let ss: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    let variance = ss / (n - 1) as f64;
    Ok(DescriptiveSummary { n, mean, variance, sd: variance.sqrt(), min, max })
}

pub fn mean(values: &[f64]) -> Result<f64, AnalysisError> {
    Ok(summarize(values)?.mean)
}

/// Type-7 empirical quantile over an ascending slice.
pub fn quantile_sorted(sorted: &[f64], probability: f64) -> Result<f64, AnalysisError> {
    assert_sample(sorted, "sorted")?;
    if !(probability >= 0.0 && probability <= 1.0) {
        return Err(AnalysisError::Domain("probability must be in [0, 1]".to_string()));
    }
    let pos = (sorted.len() - 1) as f64 * probability;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return Ok(sorted[lo]);
    }
    Ok(sorted[lo] + (pos - lo as f64) * (sorted[hi] - sorted[lo]))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WilsonInterval {
    pub lower: f64,
    pub upper: f64,
    pub center: f64,
    pub level: f64,
    pub successes: u64,
    pub n: u64,
    pub proportion: f64,
}

/// Wilson score interval, bounds clamped to [0, 1].
pub fn wilson_interval(successes: u64, n: u64, confidence: f64) -> Result<WilsonInterval, AnalysisError> {
    if n == 0 {
        return Err(AnalysisError::EmptySample("n must be greater than zero".to_string()));
    }
    if successes > n {
        return Err(AnalysisError::Domain("successes must not exceed n".to_string()));
    }
    assert_level(confidence, "confidence")?;
    let p = successes as f64 / n as f64;
    let z = normal_quantile(1.0 - (1.0 - confidence) / 2.0)?;
    let z2 = z * z;
    let n = n as f64;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let half = z / denom * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    Ok(WilsonInterval {
        lower: (center - half).max(0.0),
        upper: (center + half).min(1.0),
        center,
        level: confidence,
        successes,
        n: n as u64,
        proportion: p,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapCi {
    pub lower: f64,
    pub upper: f64,
    pub level: f64,
    pub estimate: f64,
    pub n: usize,
    pub iterations: usize,
    pub seed: String,
}

/// Bootstrap replicate means in draw order; single SeededPrng stream.
pub fn bootstrap_mean_replicates(
    values: &[f64],
    seed: &str,
    iterations: usize,
) -> Result<Vec<f64>, AnalysisError> {
    assert_sample(values, "values")?;
    if seed.is_empty() {
        return Err(AnalysisError::Domain("seed must be a non-empty string".to_string()));
    }
    if iterations == 0 {
        return Err(AnalysisError::Domain("iterations must be positive".to_string()));
    }
    let mut prng = SeededPrng::new(seed);
    let n = values.len() as u32;
    let mut out = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let mut total = 0.0;
        for _ in 0..values.len() {
            let idx = prng.next_int(n).map_err(|e| AnalysisError::Domain(e.to_string()))? as usize;
            total += values[idx];
        }
        out.push(total / values.len() as f64);
    }
    Ok(out)
}

pub fn percentile_interval(replicates: &[f64], level: f64) -> Result<ConfidenceInterval, AnalysisError> {
    assert_sample(replicates, "replicates")?;
    assert_level(level, "level")?;
    let mut sorted = replicates.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let tail = (1.0 - level) / 2.0;
    Ok(ConfidenceInterval {
        lower: quantile_sorted(&sorted, tail)?,
        upper: quantile_sorted(&sorted, 1.0 - tail)?,
        level,
    })
}

pub fn bootstrap_mean_ci(
    values: &[f64],
    seed: &str,
    iterations: usize,
    confidence: f64,
) -> Result<BootstrapCi, AnalysisError> {
    assert_level(confidence, "confidence")?;
    let replicates = bootstrap_mean_replicates(values, seed, iterations)?;
    let iv = percentile_interval(&replicates, confidence)?;
    Ok(BootstrapCi {
        lower: iv.lower,
        upper: iv.upper,
        level: confidence,
        estimate: mean(values)?,
        n: values.len(),
        iterations,
        seed: seed.to_string(),
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OneSampleT {
    pub n: usize,
    pub mean: f64,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

/// One-sample t interval; `None` bounds when n < 2 or variance is zero.
pub fn one_sample_t(values: &[f64], coverage: f64) -> Result<OneSampleT, AnalysisError> {
    assert_level(coverage, "coverage")?;
    let s = summarize(values)?;
    if s.n < 2 || !(s.sd > 0.0) {
        return Ok(OneSampleT { n: s.n, mean: s.mean, lower: None, upper: None });
    }
    let crit = crate::special::student_t_quantile(1.0 - (1.0 - coverage) / 2.0, (s.n - 1) as u32)?;
    let half = crit * s.sd / (s.n as f64).sqrt();
    Ok(OneSampleT { n: s.n, mean: s.mean, lower: Some(s.mean - half), upper: Some(s.mean + half) })
}

/// Holm-Bonferroni step-down: returns rejection flags in input order.
pub fn holm_bonferroni(p_values: &[f64], alpha: f64) -> Result<Vec<bool>, AnalysisError> {
    if !(alpha > 0.0 && alpha < 1.0) {
        return Err(AnalysisError::Domain("alpha must be in (0, 1)".to_string()));
    }
    let m = p_values.len();
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| p_values[a].partial_cmp(&p_values[b]).unwrap());
    let mut reject = vec![false; m];
    for (rank, &i) in order.iter().enumerate() {
        if p_values[i] <= alpha / (m - rank) as f64 {
            reject[i] = true;
        } else {
            break;
        }
    }
    Ok(reject)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_point_mass_exact_zero() {
        let s = summarize(&[0.5, 0.5, 0.5]).unwrap();
        assert_eq!(s.variance, 0.0);
    }

    #[test]
    fn bootstrap_is_deterministic() {
        let a = bootstrap_mean_replicates(&[1.0, 2.0, 3.0], "s", 50).unwrap();
        let b = bootstrap_mean_replicates(&[1.0, 2.0, 3.0], "s", 50).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn wilson_reference() {
        let w = wilson_interval(8, 10, 0.95).unwrap();
        assert!((w.center - 0.71674).abs() < 1e-4);
        assert!((w.lower - 0.49016).abs() < 1e-4);
        assert!((w.upper - 0.94332).abs() < 1e-4);
    }

    #[test]
    fn bootstrap_matches_ts_oracle() {
        // TS bootstrapMeanCi([0.1,0.2,0.3], seed 's', 100 reps, 0.95).
        let r = bootstrap_mean_ci(&[0.1, 0.2, 0.3], "s", 100, 0.95).unwrap();
        assert!((r.estimate - 0.20000000000000004).abs() < 1e-15);
        assert!((r.lower - 0.11583333333333343).abs() < 1e-12);
        assert!((r.upper - 0.3).abs() < 1e-12);
        assert_eq!(r.iterations, 100);
    }

    #[test]
    fn holm_basic() {
        assert_eq!(holm_bonferroni(&[0.01, 0.04, 0.5], 0.05).unwrap(), vec![true, false, false]);
    }
}
