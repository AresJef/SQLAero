//! Immutable Python compression permissions backed by the shared native options.

use crate::{config::Argument, errors, input, output};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyList, PyString, PyTuple};
use sqlaero_core::transport::compression::{Algorithm, Algorithms, Options};
use sqlaero_core::transport::deadline::IoBudget;

const WORK_BYTES: usize = 256 * 1024;

/// Immutable connection-compression permissions. Supply algorithms explicitly
/// as CSV text, a finite list/tuple of names, or None for uncompressed only.
/// Names use ASCII trimming/case folding; input order is not a priority.
/// zstd_compression_level is an exact built-in int in 1..=22, default 3, even
/// when zstd is absent. Its getter is None when zstd is not permitted.
/// A configured permission set does not establish any negotiated connection.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Compression {
    options: Options,
    algorithms: Py<PyTuple>,
}

impl Compression {
    /// Copy only already prepared native data into connection setup.
    pub(crate) fn options(&self) -> Options {
        self.options
    }

    /// Materialize known native facts without public parsing or a transport
    /// owner. Connection caches an actual single-algorithm snapshot separately;
    /// an actual uncompressed connection is represented by None at that caller.
    pub(crate) fn from_options(py: Python<'_>, options: Options) -> PyResult<Self> {
        Ok(Self {
            options,
            algorithms: names_object(py, options.algorithms())?,
        })
    }
}

#[pymethods]
impl Compression {
    #[new]
    #[pyo3(signature = (*, algorithms, zstd_compression_level=Argument::default()),
        text_signature = "(*, algorithms, zstd_compression_level=3)")]
    fn new(algorithms: &Bound<'_, PyAny>, zstd_compression_level: Argument<'_>) -> PyResult<Self> {
        let py = algorithms.py();
        let algorithms = parse_algorithms(algorithms, "algorithms", None)?;
        let level = level(zstd_compression_level.0.as_ref(), py)?;
        let options = Options::new(algorithms, level)
            .map_err(|message| errors::builtin_message::<PyValueError>(py, message))?;
        Self::from_options(py, options)
    }

    /// Normalized immutable permissions, cached as one tuple. Their stable
    /// canonical order does not select the negotiated algorithm's priority.
    #[getter]
    fn algorithms<'py>(&self, py: Python<'py>) -> Bound<'py, PyTuple> {
        self.algorithms.bind(py).clone()
    }

    /// Saved zstd level when zstd is permitted, otherwise None. Validation is
    /// still strict for every constructor call, including zlib/raw permissions.
    #[getter]
    fn zstd_compression_level(&self) -> Option<u8> {
        self.options
            .algorithms()
            .permits(Algorithm::Zstd)
            .then_some(self.options.zstd_level())
    }

    fn __repr__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        let names = self.algorithms.bind(py).repr()?;
        let names = names.to_str()?;
        let text = if self.options.algorithms().permits(Algorithm::Zstd) {
            errors::message(
                py,
                format_args!(
                    "Compression(algorithms={names}, zstd_compression_level={})",
                    self.options.zstd_level()
                ),
            )?
        } else {
            errors::message(py, format_args!("Compression(algorithms={names})"))?
        };
        PyString::from_bytes(py, text.as_bytes())
    }
    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.__repr__(py)
    }
}

/// Share the class and shorthand boundary without constructing a Python
/// Compression wrapper for shorthand or reparsing a prepared class instance.
pub(crate) fn parse(
    value: Option<&Bound<'_, PyAny>>,
    budget: Option<&IoBudget>,
) -> PyResult<Options> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(Options::default());
    };
    if let Ok(config) = value.cast::<Compression>() {
        return Ok(config.get().options());
    }
    Options::new(parse_algorithms(value, "compression", budget)?, 3)
        .map_err(|message| errors::builtin_message::<PyValueError>(value.py(), message))
}

fn level(value: Option<&Bound<'_, PyAny>>, py: Python<'_>) -> PyResult<u8> {
    let Some(value) = value else {
        return Ok(3);
    };
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(errors::builtin_message::<PyTypeError>(
            py,
            "zstd_compression_level must be an exact built-in int in 1..=22",
        ));
    }
    let range_error =
        || errors::builtin_message::<PyValueError>(py, "zstd_compression_level must be in 1..=22");
    let level = value.extract::<u8>().map_err(|error| {
        if error.is_instance_of::<PyOverflowError>(py) {
            range_error()
        } else {
            error
        }
    })?;
    if !(1..=22).contains(&level) {
        return Err(range_error());
    }
    Ok(level)
}

fn names_object(py: Python<'_>, algorithms: Algorithms) -> PyResult<Py<PyTuple>> {
    let mut values = Vec::new();
    values
        .try_reserve(3)
        .map_err(|_| errors::memory_error(py))?;
    for name in algorithms.names() {
        values.push(
            PyString::from_bytes(py, name.as_bytes())?
                .into_any()
                .unbind(),
        );
    }
    output::tuple_from_ready_vec(py, values).map(Bound::unbind)
}

fn parse_algorithms(
    value: &Bound<'_, PyAny>,
    path: &str,
    budget: Option<&IoBudget>,
) -> PyResult<Algorithms> {
    let py = value.py();
    check(py, budget)?;
    if value.is_none() {
        return Ok(Algorithms::UNCOMPRESSED);
    }
    let mut names = Vec::new();
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str()?;
        let mut start = 0;
        let mut item = 0;
        for (chunk, bytes) in text.as_bytes().chunks(WORK_BYTES).enumerate() {
            for (position, byte) in bytes.iter().enumerate() {
                if *byte == b',' {
                    let end = chunk * WORK_BYTES + position;
                    add_name(py, &text[start..end], path, item, &mut names, budget)?;
                    item += 1;
                    start = end + 1;
                }
            }
            check(py, budget)?;
        }
        add_name(py, &text[start..], path, item, &mut names, budget)?;
    } else {
        let list = value.cast::<PyList>().ok();
        let tuple = value.cast::<PyTuple>().ok();
        let length = if let Some(list) = list {
            list.len()
        } else if let Some(tuple) = tuple {
            tuple.len()
        } else {
            return Err(errors::builtin_format::<PyTypeError>(
                py,
                format_args!("{path} must be str, list or tuple of str, or None"),
            ));
        };
        for index in 0..length {
            if index.is_multiple_of(1024) {
                check(py, budget)?;
            }
            let value = if let Some(list) = list {
                list.get_item(index)?
            } else if let Some(tuple) = tuple {
                tuple.get_item(index)?
            } else {
                return Err(errors::client(
                    py,
                    sqlaero_core::error::ClientErrorCode::InternalState,
                    "compression sequence was not retained",
                ));
            };
            let name = value.cast::<PyString>().map_err(|_| {
                errors::builtin_format::<PyTypeError>(
                    py,
                    format_args!("{path}[{index}] must be str"),
                )
            })?;
            add_name(py, name.to_str()?, path, index, &mut names, budget)?;
        }
    }
    check(py, budget)?;
    Algorithms::from_names(names).map_err(|message| {
        errors::builtin_format::<PyValueError>(py, format_args!("{path}: {message}"))
    })
}

/// Bound whitespace scans as well as large CSV/list walks. The shared core
/// remains the sole authority for accepted algorithm names and permissions.
fn add_name(
    py: Python<'_>,
    name: &str,
    path: &str,
    index: usize,
    names: &mut Vec<&'static str>,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let bytes = name.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();
    let mut work = 0;
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
        work += 1;
        if work == WORK_BYTES {
            check(py, budget)?;
            work = 0;
        }
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
        work += 1;
        if work == WORK_BYTES {
            check(py, budget)?;
            work = 0;
        }
    }
    let parsed = Algorithms::from_names([&name[start..end]]).map_err(|message| {
        errors::builtin_format::<PyValueError>(py, format_args!("{path}[{index}]: {message}"))
    })?;
    for canonical in parsed.names() {
        if !names.contains(&canonical) {
            names.try_reserve(1).map_err(|_| errors::memory_error(py))?;
            names.push(canonical);
        }
    }
    Ok(())
}

fn check(py: Python<'_>, budget: Option<&IoBudget>) -> PyResult<()> {
    budget.map_or_else(|| py.check_signals(), |budget| input::check(py, budget))
}

/// Observe shared native preparation; this does not negotiate a connection.
#[pyfunction]
#[pyo3(signature = (value=None))]
pub(crate) fn _parse_compression<'py>(
    py: Python<'py>,
    value: Option<&Bound<'py, PyAny>>,
) -> PyResult<(Py<PyTuple>, u8)> {
    let options = parse(value, None)?;
    Ok((
        names_object(py, options.algorithms())?,
        options.zstd_level(),
    ))
}
