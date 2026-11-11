//! Special functions: log-gamma, normal CDF/quantile, Student-t CDF/quantile.
//!
//! Ports `packages/analysis/src/special.ts` (Cephes-style incomplete beta,
//! Acklam normal quantile, bisection t quantile).

use crate::AnalysisError;

pub fn log_gamma(x: f64) -> f64 {
    // Lanczos approximation, g=7, n=9.
    const C: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        // Reflection formula.
        return (std::f64::consts::PI / ((std::f64::consts::PI * x).sin())).ln() - log_gamma(1.0 - x);
    }
    let x = x - 1.0;
    let mut a = C[0];
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    let t = x + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Regularized incomplete beta I_x(a, b) via continued fraction.
pub fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> Result<f64, AnalysisError> {
    if !(x >= 0.0 && x <= 1.0) {
        return Err(AnalysisError::Domain("x must be in [0, 1]".to_string()));
    }
    if a <= 0.0 || b <= 0.0 {
        return Err(AnalysisError::Domain("a and b must be positive".to_string()));
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    if x == 1.0 {
        return Ok(1.0);
    }
    let lbeta = log_gamma(a) + log_gamma(b) - log_gamma(a + b);
    if x < (a + 1.0) / (a + b + 2.0) {
        Ok((a * x.ln() + b * (1.0 - x).ln() - lbeta).exp() * beta_cf(x, a, b) / a)
    } else {
        Ok(1.0
            - (a * x.ln() + b * (1.0 - x).ln() - lbeta).exp() * beta_cf(1.0 - x, b, a) / b)
    }
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    const MAX_IT: usize = 200;
    const EPS: f64 = 3e-14;
    const FPMIN: f64 = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAX_IT {
        let m = m as f64;
        let m2 = 2.0 * m;
        let mut aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

fn erf(z: f64) -> f64 {
    // Abramowitz-Stegun 7.1.26.
    let t = 1.0 / (1.0 + 0.3275911 * z.abs());
    let y = 1.0
        - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-z * z).exp();
    if z >= 0.0 { y } else { -y }
}

#[must_use]
pub fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// Acklam's inverse normal CDF approximation.
pub fn normal_quantile(p: f64) -> Result<f64, AnalysisError> {
    if !(p > 0.0 && p < 1.0) {
        return Err(AnalysisError::Domain("p must be in (0, 1)".to_string()));
    }
    const A: [f64; 6] = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383577518672690e+02,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    const B: [f64; 5] = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    const C: [f64; 6] = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    const D: [f64; 4] = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let plow = 0.02425;
    let phigh = 1.0 - plow;
    if p < plow {
        let q = (-2.0 * p.ln()).sqrt();
        return Ok(
            (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
                / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0),
        );
    }
    if p > phigh {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        return Ok(
            -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
                / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0),
        );
    }
    let q = p - 0.5;
    let r = q * q;
    Ok(
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0),
    )
}

/// Student-t CDF via the regularized incomplete beta relation.
pub fn student_t_cdf(t: f64, df: f64) -> Result<f64, AnalysisError> {
    if df <= 0.0 {
        return Err(AnalysisError::Domain("df must be positive".to_string()));
    }
    let x = df / (df + t * t);
    let ib = regularized_incomplete_beta(x, df / 2.0, 0.5)?;
    if t >= 0.0 {
        Ok(1.0 - 0.5 * ib)
    } else {
        Ok(0.5 * ib)
    }
}

/// Student-t quantile by bisection on the CDF.
pub fn student_t_quantile(p: f64, df: u32) -> Result<f64, AnalysisError> {
    if !(p > 0.0 && p < 1.0) {
        return Err(AnalysisError::Domain("p must be in (0, 1)".to_string()));
    }
    if df == 0 {
        return Err(AnalysisError::Domain("df must be positive".to_string()));
    }
    let df = f64::from(df);
    let mut lo = -1e6f64;
    let mut hi = 1e6f64;
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let c = student_t_cdf(mid, df)?;
        if c < p {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-12 {
            break;
        }
    }
    Ok(0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_quantile_median() {
        assert!((normal_quantile(0.5).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn normal_quantile_975() {
        assert!((normal_quantile(0.975).unwrap() - 1.959964).abs() < 1e-5);
    }

    #[test]
    fn t_quantile_df19_975() {
        // Reference: qt(0.975, 19) = 2.093024.
        assert!((student_t_quantile(0.975, 19).unwrap() - 2.093024).abs() < 1e-4);
    }

    #[test]
    fn wilson_reference_point() {
        // k=8, n=10 at 95%: [0.49016, 0.94332], centre 0.71674.
        let z = normal_quantile(0.975).unwrap();
        let (p, n) = (0.8f64, 10f64);
        let denom = 1.0 + z * z / n;
        let center = (p + z * z / (2.0 * n)) / denom;
        let half = z / denom * ((p * (1.0 - p)) / n + z * z / (4.0 * n * n)).sqrt();
        assert!((center - 0.71674).abs() < 1e-4);
        assert!((center - half - 0.49016).abs() < 1e-4);
        assert!((center + half - 0.94332).abs() < 1e-4);
    }
}
