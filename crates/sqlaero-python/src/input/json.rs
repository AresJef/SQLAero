//! JSON wrapper and native serialization at the single input boundary.

use super::primitive::{self, Source};
use super::sequence::Sequence;
use super::sink::Destination;
use super::{ScalarTypes, integer};
use crate::{config, errors};
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt};
use sqlaero_core::bind::Context;
use sqlaero_core::json::{Error, Options, Writer};
use sqlaero_core::transport::deadline::IoBudget;
use sqlaero_transcode::value::{Consumer, ScalarRef};

/// A lazy JSON text parameter. The payload reference and allow_nan are read-only;
/// payload contents may be changed between calls. Serialization happens during
/// Escape, under the current call's ownership and error checks.
#[pyclass(name = "JSON", module = "sqlaero._sqlaero")]
pub(crate) struct Json {
    data: Option<Py<PyAny>>,
    allow_nan: bool,
}

#[pymethods]
impl Json {
    #[new]
    #[pyo3(signature = (obj, *, allow_nan=config::Argument::default()), text_signature = "(obj, *, allow_nan=True)")]
    fn new(obj: Py<PyAny>, allow_nan: config::Argument<'_>) -> PyResult<Self> {
        Ok(Self {
            data: Some(obj),
            allow_nan: option(allow_nan)?,
        })
    }

    /// The original read-only payload reference; no serialized answer is cached.
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyAny> {
        self.data
            .as_ref()
            .map_or_else(|| py.None(), |value| value.clone_ref(py))
    }

    /// Nonfinite-number policy; default True writes null, False raises ValueError.
    #[getter]
    fn allow_nan(&self) -> bool {
        self.allow_nan
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.data)
    }
    fn __clear__(&mut self) {
        self.data = None;
    }
}

fn option(argument: config::Argument<'_>) -> PyResult<bool> {
    argument
        .0
        .as_ref()
        .map_or(Ok(true), |value| config::boolean(value, "allow_nan"))
}

fn failure(py: Python<'_>, error: Error) -> PyErr {
    match error {
        Error::Unsupported(name) => errors::builtin_format::<PyTypeError>(
            py,
            format_args!("JSON serialization does not support {name}"),
        ),
        Error::NonFinite | Error::Depth => {
            errors::builtin_format::<PyValueError>(py, format_args!("{error}"))
        }
        Error::Capacity(_) => errors::memory_error(py),
        Error::State(_) => errors::client_format(
            py,
            sqlaero_core::error::ClientErrorCode::InternalState,
            format_args!("{error}"),
        ),
    }
}

struct Serialize<'a> {
    types: &'a ScalarTypes,
    options: Options,
    budget: Option<&'a IoBudget>,
    writer: Writer,
    active: Vec<usize>,
    path: Vec<usize>,
}

impl Serialize<'_> {
    fn value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = value.py();
        if value.is_instance_of::<Json>() {
            return Err(errors::builtin_message::<PyTypeError>(
                py,
                "JSON wrappers cannot be serialized inside JSON; use wrapped.data explicitly",
            ));
        }
        if let Some(source) = primitive::read(value)? {
            return match source {
                Source::Native(value) => self
                    .writer
                    .write(value, &self.options)
                    .map_err(|error| failure(py, error)),
                Source::Text(source) => super::unicode::write(
                    source,
                    &mut self.writer,
                    &self.options,
                    self.budget,
                    "utf-8",
                    |error, _, _| failure(py, error),
                ),
                Source::WideInteger { value, overflow } if overflow > 0 => {
                    match value.extract::<u64>() {
                        Ok(value) => self
                            .writer
                            .write(ScalarRef::Unsigned(value), &self.options)
                            .map_err(|error| failure(py, error)),
                        Err(error) if error.is_instance_of::<PyOverflowError>(py) => {
                            self.integer(value)
                        }
                        Err(error) => Err(error),
                    }
                }
                Source::WideInteger { value, .. } => self.integer(value),
            };
        }
        if let Some(sequence) = Sequence::plain(value)? {
            let identity = value.as_ptr() as usize;
            if self.active.contains(&identity) {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "JSON input contains a true cycle",
                ));
            }
            self.active
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            self.active.push(identity);
            self.writer
                .array_start()
                .map_err(|error| failure(py, error))?;
            for index in 0..sequence.len() {
                if index % 256 == 0 {
                    super::checkpoint(py, self.budget)?;
                }
                self.path
                    .try_reserve(1)
                    .map_err(|_| errors::memory_error(py))?;
                self.path.push(index);
                self.value(&sequence.row_item(index)?)?;
                self.path.pop();
            }
            self.writer
                .array_end()
                .map_err(|error| failure(py, error))?;
            self.active.pop();
            return Ok(());
        }
        let detail = self.types.diagnostic(value)?;
        Err(errors::builtin_format::<PyTypeError>(
            py,
            format_args!("unsupported JSON input: {detail}"),
        ))
    }

    fn integer(&mut self, source: &Bound<'_, PyInt>) -> PyResult<()> {
        let py = source.py();
        super::checkpoint(py, self.budget)?;
        let owner = integer::Export::new(source)?;
        let (negative, source) = match owner.value()? {
            integer::Value::Compact(value) => {
                return self
                    .writer
                    .write(ScalarRef::Signed(value), &self.options)
                    .map_err(|error| failure(py, error));
            }
            integer::Value::Digits { negative, source } => (negative, source),
        };
        let mut decimal = sqlaero_transcode::integer::DecimalDigits::new(source)
            .map_err(|error| integer::failure(py, error))?;
        // Copy, arithmetic and output all return to the same original control
        // boundary; no Python decimal string or unbounded bigint conversion.
        while !decimal
            .step(4096)
            .map_err(|error| integer::failure(py, error))?
        {
            super::checkpoint(py, self.budget)?;
        }
        super::checkpoint(py, self.budget)?;
        let groups = decimal.groups().ok_or_else(|| {
            errors::client(
                py,
                sqlaero_core::error::ClientErrorCode::InternalState,
                "native integer conversion finished without complete decimal digits",
            )
        })?;
        self.writer
            .integer_start(negative)
            .map_err(|error| failure(py, error))?;
        for (index, group) in groups.iter().rev().enumerate() {
            if index.is_multiple_of(1024) {
                super::checkpoint(py, self.budget)?;
            }
            let mut format = itoa::Buffer::new();
            let digits = format.format(*group).as_bytes();
            if index != 0 && digits.len() < 9 {
                self.writer
                    .integer_fragment(&b"000000000"[..9 - digits.len()])
                    .map_err(|error| failure(py, error))?;
            }
            self.writer
                .integer_fragment(digits)
                .map_err(|error| failure(py, error))?;
        }
        self.writer
            .integer_end()
            .map_err(|error| failure(py, error))?;
        super::checkpoint(py, self.budget)
    }
}

struct Path<'a>(&'a [usize]);
impl std::fmt::Display for Path<'_> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("data")?;
        for index in self.0 {
            write!(output, "[{index}]")?;
        }
        Ok(())
    }
}

pub(crate) fn serialize(
    value: &Bound<'_, PyAny>,
    types: &ScalarTypes,
    allow_nan: bool,
    budget: Option<&IoBudget>,
) -> PyResult<Vec<u8>> {
    let py = value.py();
    super::checkpoint(py, budget)?;
    let mut state = Serialize {
        types,
        options: Options { allow_nan },
        budget,
        writer: Writer::new(),
        active: Vec::new(),
        path: Vec::new(),
    };
    if let Err(error) = state.value(value) {
        errors::note_format(
            py,
            &error,
            format_args!("while serializing JSON at {}", Path(&state.path)),
        );
        return Err(error);
    }
    super::checkpoint(py, budget)?;
    state.writer.finish().map_err(|error| failure(py, error))
}

/// Serialize one lazy wrapper as text, never as source SQL or binary input.
pub(crate) fn mysql<D: Destination + ?Sized>(
    value: &Bound<'_, Json>,
    types: &ScalarTypes,
    output: &mut D,
    context: &Context,
    budget: Option<&IoBudget>,
) -> PyResult<()> {
    let py = value.py();
    let (data, allow_nan) = {
        let value = value.try_borrow()?;
        (value.data(py), value.allow_nan)
    };
    let bytes = serialize(data.bind(py), types, allow_nan, budget)?;
    // The writer only emits valid UTF-8. Keep validation fallible rather than
    // widening the unsafe boundary; no Python JSON str/bytes is created here.
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        errors::client(
            py,
            sqlaero_core::error::ClientErrorCode::InternalState,
            "native JSON writer produced invalid UTF-8",
        )
    })?;
    super::text_utf8(py, text, output, context, budget)
}

/// Serialize obj as compact UTF-8 JSON and return built-in bytes. allow_nan
/// must be a keyword-only bool; default True maps nonfinite numbers to null, False
/// raises ValueError. JSON wrappers are rejected; pass wrapped.data explicitly.
#[pyfunction]
#[pyo3(pass_module, signature = (obj, *, allow_nan=config::Argument::default()), text_signature = "($module, obj, *, allow_nan=True)")]
pub(crate) fn json_dumps<'py>(
    module: &Bound<'py, PyModule>,
    obj: &Bound<'py, PyAny>,
    allow_nan: config::Argument<'py>,
) -> PyResult<Bound<'py, PyBytes>> {
    let py = module.py();
    let owner = module.getattr(c"_scalar_types")?;
    let types = owner.extract::<PyRef<'_, ScalarTypes>>()?;
    let bytes = serialize(obj, &types, option(allow_nan)?, None)?;
    Ok(crate::output::bytes_bounded(py, &bytes, None)?.cast_into::<PyBytes>()?)
}
