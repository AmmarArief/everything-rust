//! Python bindings for `src/colorsys_pkg/`: generated from the signatures in `port.rs` by carcinize.
//! Regenerate with `carcinize refresh` after changing them.

use pyo3::prelude::*;

pub mod js;
pub mod port;

fn error(message: String) -> PyErr {
    use pyo3::exceptions as e;
    let (kind, text) = message.split_once(": ").unwrap_or(("", message.as_str()));
    let text = text.to_string();
    match kind {
        "TypeError" => e::PyTypeError::new_err(text),
        "KeyError" => e::PyKeyError::new_err(text),
        "IndexError" => e::PyIndexError::new_err(text),
        "OverflowError" => e::PyOverflowError::new_err(text),
        "ZeroDivisionError" => e::PyZeroDivisionError::new_err(text),
        "AttributeError" => e::PyAttributeError::new_err(text),
        "RuntimeError" => e::PyRuntimeError::new_err(text),
        "NotImplementedError" => e::PyNotImplementedError::new_err(text),
        "LookupError" => e::PyLookupError::new_err(text),
        "ArithmeticError" => e::PyArithmeticError::new_err(text),
        "UnicodeError" => e::PyUnicodeError::new_err(text),
        "ValueError" => e::PyValueError::new_err(text),
        _ => e::PyValueError::new_err(message),
    }
}

fn rgb_to_yiq_inner(r: String, g: String, b: String) -> Result<String, String> {
    let r = crate::js::from_json(&r)?;
    let g = crate::js::from_json(&g)?;
    let b = crate::js::from_json(&b)?;
    let out = port::rgb_to_yiq(r, g, b)?;
    crate::js::to_json(&out)
}

/// `rgb_to_yiq`.
#[pyfunction]
#[pyo3(name = "rgb_to_yiq", signature = (r, g, b))]
fn rgb_to_yiq(r: String, g: String, b: String) -> PyResult<String> {
    rgb_to_yiq_inner(r, g, b).map_err(error)
}

fn yiq_to_rgb_inner(y: String, i: String, q: String) -> Result<String, String> {
    let y = crate::js::from_json(&y)?;
    let i = crate::js::from_json(&i)?;
    let q = crate::js::from_json(&q)?;
    let out = port::yiq_to_rgb(y, i, q)?;
    crate::js::to_json(&out)
}

/// `yiq_to_rgb`.
#[pyfunction]
#[pyo3(name = "yiq_to_rgb", signature = (y, i, q))]
fn yiq_to_rgb(y: String, i: String, q: String) -> PyResult<String> {
    yiq_to_rgb_inner(y, i, q).map_err(error)
}

fn rgb_to_hls_inner(r: String, g: String, b: String) -> Result<String, String> {
    let r = crate::js::from_json(&r)?;
    let g = crate::js::from_json(&g)?;
    let b = crate::js::from_json(&b)?;
    let out = port::rgb_to_hls(r, g, b)?;
    crate::js::to_json(&out)
}

/// `rgb_to_hls`.
#[pyfunction]
#[pyo3(name = "rgb_to_hls", signature = (r, g, b))]
fn rgb_to_hls(r: String, g: String, b: String) -> PyResult<String> {
    rgb_to_hls_inner(r, g, b).map_err(error)
}

fn hls_to_rgb_inner(h: String, l: String, s: String) -> Result<String, String> {
    let h = crate::js::from_json(&h)?;
    let l = crate::js::from_json(&l)?;
    let s = crate::js::from_json(&s)?;
    let out = port::hls_to_rgb(h, l, s)?;
    crate::js::to_json(&out)
}

/// `hls_to_rgb`.
#[pyfunction]
#[pyo3(name = "hls_to_rgb", signature = (h, l, s))]
fn hls_to_rgb(h: String, l: String, s: String) -> PyResult<String> {
    hls_to_rgb_inner(h, l, s).map_err(error)
}

fn rgb_to_hsv_inner(r: String, g: String, b: String) -> Result<String, String> {
    let r = crate::js::from_json(&r)?;
    let g = crate::js::from_json(&g)?;
    let b = crate::js::from_json(&b)?;
    let out = port::rgb_to_hsv(r, g, b)?;
    crate::js::to_json(&out)
}

/// `rgb_to_hsv`.
#[pyfunction]
#[pyo3(name = "rgb_to_hsv", signature = (r, g, b))]
fn rgb_to_hsv(r: String, g: String, b: String) -> PyResult<String> {
    rgb_to_hsv_inner(r, g, b).map_err(error)
}

fn hsv_to_rgb_inner(h: String, s: String, v: String) -> Result<String, String> {
    let h = crate::js::from_json(&h)?;
    let s = crate::js::from_json(&s)?;
    let v = crate::js::from_json(&v)?;
    let out = port::hsv_to_rgb(h, s, v)?;
    crate::js::to_json(&out)
}

/// `hsv_to_rgb`.
#[pyfunction]
#[pyo3(name = "hsv_to_rgb", signature = (h, s, v))]
fn hsv_to_rgb(h: String, s: String, v: String) -> PyResult<String> {
    hsv_to_rgb_inner(h, s, v).map_err(error)
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rgb_to_yiq, m)?)?;
    m.add_function(wrap_pyfunction!(yiq_to_rgb, m)?)?;
    m.add_function(wrap_pyfunction!(rgb_to_hls, m)?)?;
    m.add_function(wrap_pyfunction!(hls_to_rgb, m)?)?;
    m.add_function(wrap_pyfunction!(rgb_to_hsv, m)?)?;
    m.add_function(wrap_pyfunction!(hsv_to_rgb, m)?)?;
    Ok(())
}
