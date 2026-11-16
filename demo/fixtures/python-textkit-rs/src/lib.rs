//! Python bindings for `src/textkit/`: generated from the signatures in `port.rs` by carcinize.
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

fn slugify_inner(
    text: String,
    sep: Option<String>,
    max_length: Option<i64>,
) -> Result<String, String> {
    let out = port::slugify(text, sep, max_length)?;
    Ok(out)
}

/// `slugify`.
#[pyfunction]
#[pyo3(name = "slugify", signature = (text, sep=String::from("-"), *, max_length=None))]
fn slugify(text: String, sep: Option<String>, max_length: Option<i64>) -> PyResult<String> {
    slugify_inner(text, sep, max_length).map_err(error)
}

fn word_counts_inner(text: String, min_length: i64) -> Result<String, String> {
    let out = port::word_counts(text, min_length)?;
    crate::js::to_json(&out)
}

/// `word_counts`.
#[pyfunction]
#[pyo3(name = "word_counts", signature = (text, min_length=1))]
fn word_counts(text: String, min_length: i64) -> PyResult<String> {
    word_counts_inner(text, min_length).map_err(error)
}

fn chunk_inner(items: Vec<i64>, size: i64) -> Result<String, String> {
    let out = port::chunk(items, size)?;
    crate::js::to_json(&out)
}

/// `chunk`.
#[pyfunction]
#[pyo3(name = "chunk", signature = (items, size))]
fn chunk(items: Vec<i64>, size: i64) -> PyResult<String> {
    chunk_inner(items, size).map_err(error)
}

fn page_count_inner(total: i64, per_page: i64) -> Result<i64, String> {
    let out = port::page_count(total, per_page)?;
    Ok(out)
}

/// `page_count`.
#[pyfunction]
#[pyo3(name = "page_count", signature = (total, per_page))]
fn page_count(total: i64, per_page: i64) -> PyResult<i64> {
    page_count_inner(total, per_page).map_err(error)
}

fn mean_inner(values: Vec<f64>) -> Result<f64, String> {
    let out = port::mean(values)?;
    Ok(out)
}

/// `mean`.
#[pyfunction]
#[pyo3(name = "mean", signature = (values))]
fn mean(values: Vec<f64>) -> PyResult<f64> {
    mean_inner(values).map_err(error)
}

fn stack_new_inner(capacity: i64) -> Result<port::Stack, String> {
    let out = port::Stack::new(capacity)?;
    Ok(out)
}

fn stack_push_inner(inner: &mut port::Stack, item: String) -> Result<i64, String> {
    let out = inner.push(item)?;
    Ok(out)
}

fn stack_pop_inner(inner: &mut port::Stack) -> Result<Option<String>, String> {
    let out = inner.pop()?;
    Ok(out)
}

fn stack_peek_inner(inner: &port::Stack) -> Result<Option<String>, String> {
    let out = inner.peek()?;
    Ok(out)
}

fn stack_size_inner(inner: &port::Stack) -> Result<i64, String> {
    let out = inner.size()?;
    Ok(out)
}

/// Class `Stack`.
#[pyclass(name = "Stack")]
pub struct Stack {
    inner: port::Stack,
}

#[pymethods]
impl Stack {
    #[new]
    #[pyo3(signature = (capacity=8))]
    fn new(capacity: i64) -> PyResult<Self> {
        stack_new_inner(capacity)
            .map(|inner| Self { inner })
            .map_err(error)
    }

    #[pyo3(name = "push", signature = (item))]
    fn push(&mut self, item: String) -> PyResult<i64> {
        stack_push_inner(&mut self.inner, item).map_err(error)
    }

    #[pyo3(name = "pop", signature = ())]
    fn pop(&mut self) -> PyResult<Option<String>> {
        stack_pop_inner(&mut self.inner).map_err(error)
    }

    #[pyo3(name = "peek", signature = ())]
    fn peek(&self) -> PyResult<Option<String>> {
        stack_peek_inner(&self.inner).map_err(error)
    }

    #[pyo3(name = "size", signature = ())]
    fn size(&self) -> PyResult<i64> {
        stack_size_inner(&self.inner).map_err(error)
    }
}

#[pymodule]
#[pyo3(name = "_native")]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(slugify, m)?)?;
    m.add_function(wrap_pyfunction!(word_counts, m)?)?;
    m.add_function(wrap_pyfunction!(chunk, m)?)?;
    m.add_function(wrap_pyfunction!(page_count, m)?)?;
    m.add_function(wrap_pyfunction!(mean, m)?)?;
    m.add_class::<Stack>()?;
    Ok(())
}
