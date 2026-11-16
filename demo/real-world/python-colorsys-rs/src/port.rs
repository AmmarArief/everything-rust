//! The Rust port of Python's `colorsys` (CPython 3.13, `Lib/colorsys.py`).
//! Translated from CPython (PSF License Version 2).
//!
//! `colorsys` has no type hints and Python callers pass ints as often as floats; ints stay ints
//! through `+`, `-`, `*` and come back out (`hsv_to_rgb(0, 0, 7)` is `(7, 7, 7)`). So the port
//! computes with `Num`, Python's int-or-float, instead of `f64`.

#![forbid(unsafe_code)]

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
enum Num {
    Int(i128),
    Float(f64),
}

use Num::{Float, Int};

impl Num {
    fn parse(v: &Value) -> Result<Self, String> {
        let bad = || "TypeError: unsupported operand type".to_string();
        match v {
            Value::Number(n) => match (n.as_i64(), n.as_f64()) {
                (Some(i), _) if !n.is_f64() => Ok(Int(i128::from(i))),
                (_, Some(x)) => Ok(Float(x)),
                _ => Err(bad()),
            },
            Value::Bool(b) => Ok(Int(i128::from(*b))),
            Value::Object(o) => {
                if let Some(text) = o.get("$int").and_then(Value::as_str) {
                    return text.parse().map(Int).map_err(|_| bad());
                }
                match o.get("$num").and_then(Value::as_str) {
                    Some("NaN") => Ok(Float(f64::NAN)),
                    Some("Infinity") => Ok(Float(f64::INFINITY)),
                    Some("-Infinity") => Ok(Float(f64::NEG_INFINITY)),
                    Some("-0") => Ok(Float(-0.0)),
                    _ => Err(bad()),
                }
            }
            _ => Err(bad()),
        }
    }

    fn f(self) -> f64 {
        match self {
            Int(i) => i as f64,
            Float(x) => x,
        }
    }

    /// The value as the Python bridge encodes it.
    fn value(self) -> Value {
        match self {
            Int(i) => match i64::try_from(i) {
                Ok(small) if small.unsigned_abs() < (1 << 53) => json!(small),
                _ => json!({"$int": i.to_string()}),
            },
            Float(x) if x.is_nan() => json!({"$num": "NaN"}),
            Float(x) if x.is_infinite() => {
                json!({"$num": if x > 0.0 { "Infinity" } else { "-Infinity" }})
            }
            Float(x) if x == 0.0 && x.is_sign_negative() => json!({"$num": "-0"}),
            Float(x) => json!(x),
        }
    }

    fn op(
        self,
        other: Self,
        int: fn(i128, i128) -> Option<i128>,
        float: fn(f64, f64) -> f64,
    ) -> Self {
        match (self, other) {
            (Int(a), Int(b)) => int(a, b).map_or_else(|| Float(float(a as f64, b as f64)), Int),
            (a, b) => Float(float(a.f(), b.f())),
        }
    }

    fn add(self, o: Self) -> Self {
        self.op(o, i128::checked_add, |a, b| a + b)
    }

    fn sub(self, o: Self) -> Self {
        self.op(o, i128::checked_sub, |a, b| a - b)
    }

    fn mul(self, o: Self) -> Self {
        self.op(o, i128::checked_mul, |a, b| a * b)
    }

    /// `/`: always a float.
    fn div(self, o: Self) -> Result<Self, String> {
        if o.f() == 0.0 {
            return Err(match (self, o) {
                (Int(_), Int(_)) => "ZeroDivisionError: division by zero".into(),
                _ => "ZeroDivisionError: float division by zero".into(),
            });
        }
        Ok(Float(self.f() / o.f()))
    }

    fn lt(self, o: Self) -> bool {
        match (self, o) {
            (Int(a), Int(b)) => a < b,
            (a, b) => a.f() < b.f(),
        }
    }

    fn eq(self, o: Self) -> bool {
        match (self, o) {
            (Int(a), Int(b)) => a == b,
            (a, b) => a.f() == b.f(),
        }
    }
}

const ONE_THIRD: f64 = 1.0 / 3.0;
const ONE_SIXTH: f64 = 1.0 / 6.0;
const TWO_THIRD: f64 = 2.0 / 3.0;

fn tuple(items: [Num; 3]) -> Value {
    json!({"$tuple": items.map(Num::value)})
}

fn args(values: [&Value; 3]) -> Result<[Num; 3], String> {
    Ok([
        Num::parse(values[0])?,
        Num::parse(values[1])?,
        Num::parse(values[2])?,
    ])
}

/// Python's `max(a, b, c)`: the first value no later value is greater than (NaN stays).
fn py_max(v: [Num; 3]) -> Num {
    v[1..]
        .iter()
        .fold(v[0], |m, &x| if m.lt(x) { x } else { m })
}

fn py_min(v: [Num; 3]) -> Num {
    v[1..]
        .iter()
        .fold(v[0], |m, &x| if x.lt(m) { x } else { m })
}

/// Python's float `x % 1.0`: in [0, 1), a zero result is +0.0.
fn mod_one(x: f64) -> f64 {
    let m = x % 1.0;
    if m != 0.0 {
        if m < 0.0 { m + 1.0 } else { m }
    } else {
        0.0
    }
}

/// Port of `rgb_to_yiq` (`colorsys.py:40`).
pub fn rgb_to_yiq(r: Value, g: Value, b: Value) -> Result<Value, String> {
    let [r, g, b] = args([&r, &g, &b])?;
    let c = Float;
    let y = c(0.30).mul(r).add(c(0.59).mul(g)).add(c(0.11).mul(b));
    let i = c(0.74).mul(r.sub(y)).sub(c(0.27).mul(b.sub(y)));
    let q = c(0.48).mul(r.sub(y)).add(c(0.41).mul(b.sub(y)));
    Ok(tuple([y, i, q]))
}

/// Port of `yiq_to_rgb` (`colorsys.py:46`): clamps like the source, so NaN passes through.
pub fn yiq_to_rgb(y: Value, i: Value, q: Value) -> Result<Value, String> {
    let [y, i, q] = args([&y, &i, &q])?;
    let c = Float;
    let clamp = |v: Num| {
        let v = if v.lt(c(0.0)) { c(0.0) } else { v };
        if c(1.0).lt(v) { c(1.0) } else { v }
    };
    let r = y
        .add(c(0.946_882_217_090_069_3).mul(i))
        .add(c(0.623_556_581_986_143_3).mul(q));
    let g = y
        .sub(c(0.274_787_646_298_978_34).mul(i))
        .sub(c(0.635_691_079_187_380_1).mul(q));
    let b = y
        .sub(c(1.108_545_034_642_032_2).mul(i))
        .add(c(1.709_006_928_406_466_6).mul(q));
    Ok(tuple([clamp(r), clamp(g), clamp(b)]))
}

/// The hue shared by `rgb_to_hls` and `rgb_to_hsv`.
fn hue([r, g, b]: [Num; 3], maxc: Num, rangec: Num) -> Result<Num, String> {
    let rc = maxc.sub(r).div(rangec)?;
    let gc = maxc.sub(g).div(rangec)?;
    let bc = maxc.sub(b).div(rangec)?;
    let h = if r.eq(maxc) {
        bc.sub(gc)
    } else if g.eq(maxc) {
        Float(2.0).add(rc).sub(bc)
    } else {
        Float(4.0).add(gc).sub(rc)
    };
    Ok(Float(mod_one(h.div(Float(6.0))?.f())))
}

/// Port of `rgb_to_hls` (`colorsys.py:76`).
pub fn rgb_to_hls(r: Value, g: Value, b: Value) -> Result<Value, String> {
    let rgb = args([&r, &g, &b])?;
    let (maxc, minc) = (py_max(rgb), py_min(rgb));
    let sumc = maxc.add(minc);
    let rangec = maxc.sub(minc);
    let l = sumc.div(Float(2.0))?;
    if minc.eq(maxc) {
        return Ok(tuple([Float(0.0), l, Float(0.0)]));
    }
    let s = if !Float(0.5).lt(l) {
        rangec.div(sumc)?
    } else {
        rangec.div(Float(2.0).sub(maxc).sub(minc))?
    };
    Ok(tuple([hue(rgb, maxc, rangec)?, l, s]))
}

/// Port of `hls_to_rgb` (`colorsys.py:99`).
pub fn hls_to_rgb(h: Value, l: Value, s: Value) -> Result<Value, String> {
    let [h, l, s] = args([&h, &l, &s])?;
    if s.eq(Float(0.0)) {
        return Ok(tuple([l, l, l]));
    }
    let m2 = if !Float(0.5).lt(l) {
        l.mul(Float(1.0).add(s))
    } else {
        l.add(s).sub(l.mul(s))
    };
    let m1 = Float(2.0).mul(l).sub(m2);
    Ok(tuple([
        v(m1, m2, h.add(Float(ONE_THIRD))),
        v(m1, m2, h),
        v(m1, m2, h.sub(Float(ONE_THIRD))),
    ]))
}

fn v(m1: Num, m2: Num, hue: Num) -> Num {
    let hue = Float(mod_one(hue.f()));
    if hue.lt(Float(ONE_SIXTH)) {
        m1.add(m2.sub(m1).mul(hue).mul(Float(6.0)))
    } else if hue.lt(Float(0.5)) {
        m2
    } else if hue.lt(Float(TWO_THIRD)) {
        m1.add(m2.sub(m1).mul(Float(TWO_THIRD).sub(hue)).mul(Float(6.0)))
    } else {
        m1
    }
}

/// Port of `rgb_to_hsv` (`colorsys.py:126`).
pub fn rgb_to_hsv(r: Value, g: Value, b: Value) -> Result<Value, String> {
    let rgb = args([&r, &g, &b])?;
    let (maxc, minc) = (py_max(rgb), py_min(rgb));
    let rangec = maxc.sub(minc);
    if minc.eq(maxc) {
        return Ok(tuple([Float(0.0), Float(0.0), maxc]));
    }
    let s = rangec.div(maxc)?;
    Ok(tuple([hue(rgb, maxc, rangec)?, s, maxc]))
}

/// Port of `hsv_to_rgb` (`colorsys.py:145`).
pub fn hsv_to_rgb(h: Value, s: Value, v: Value) -> Result<Value, String> {
    let [h, s, v] = args([&h, &s, &v])?;
    if s.eq(Float(0.0)) {
        return Ok(tuple([v, v, v]));
    }
    let scaled = h.mul(Float(6.0)).f();
    if scaled.is_nan() {
        return Err("ValueError: cannot convert float NaN to integer".into());
    }
    if scaled.is_infinite() {
        return Err("OverflowError: cannot convert float infinity to integer".into());
    }
    let i = scaled.trunc();
    let f = Float(scaled - i);
    let one = Float(1.0);
    let p = v.mul(one.sub(s));
    let q = v.mul(one.sub(s.mul(f)));
    let t = v.mul(one.sub(s.mul(one.sub(f))));
    // Python's `i % 6` on an int is never negative.
    Ok(match (i % 6.0 + 6.0) % 6.0 {
        0.0 => tuple([v, t, p]),
        1.0 => tuple([q, v, p]),
        2.0 => tuple([p, v, t]),
        3.0 => tuple([p, q, v]),
        4.0 => tuple([t, p, v]),
        _ => tuple([v, p, q]),
    })
}
