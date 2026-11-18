/// JS `Number.prototype.toString()` (what `${n}` and `String(n)` print): `1e+21`, `5e-324`,
/// `0` for `-0`, `NaN`, `Infinity`, and the shortest digits that read back as the same number.
#[allow(dead_code)]
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let shortest = format!("{:e}", x.abs());
    let k = shortest
        .split('e')
        .next()
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_digit)
        .count();
    let sci = format!("{:.*e}", k.saturating_sub(1), x.abs());
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i64;
    let n = exponent.parse::<i64>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        format!("{int}.{frac}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let mantissa = if k == 1 {
            digits.clone()
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{mantissa}e{}{}", if e < 0 { "-" } else { "+" }, e.abs())
    };
    format!("{sign}{body}")
}

/// JS `Math.round`: the nearest integer, halves toward positive infinity (`-2.5` gives `-2`,
/// where Rust's `round` gives `-3`), and `-0` for negatives that round to zero.
#[allow(dead_code)]
pub fn math_round(x: f64) -> f64 {
    if !x.is_finite() || x == x.trunc() {
        return x;
    }
    let floor = x.floor();
    let r = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if r == 0.0 && x < 0.0 { -0.0 } else { r }
}

#[cfg(test)]
mod js_number_tests {
    use super::*;

    #[test]
    fn prints_numbers_as_js_does() {
        for (x, js) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-7.5, "-7.5"),
            (0.1, "0.1"),
            (0.30000000000000004, "0.30000000000000004"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e-7, "1e-7"),
            (0.000001, "0.000001"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (2.5e-7, "2.5e-7"),
            (153596401214599.62, "153596401214599.62"),
            (-25.728225708007812, "-25.728225708007812"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(number_to_string(x), js, "{x:?}");
        }
    }

    #[test]
    fn rounds_as_math_round() {
        for (x, js) in [
            (2.5, 3.0),
            (-2.5, -2.0),
            (-2.6, -3.0),
            (0.49999999999999994, 0.0),
            (1.5, 2.0),
            (-0.4, -0.0),
            (4503599627370495.5, 4503599627370496.0),
            (-7.0, -7.0),
        ] {
            let r = math_round(x);
            assert_eq!(r.to_bits(), f64::to_bits(js), "{x}: {r}");
        }
    }
}
