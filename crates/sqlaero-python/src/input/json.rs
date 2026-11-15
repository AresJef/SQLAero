//! JSON wrapper and native serialization at the single input boundary.

use super::primitive::{self, Source};
use super::sequence::{Kind, Projection};
use std::collections::{HashMap, hash_map::RandomState};

mod keys;
mod output;
mod source;
use super::cell::NativeFailure;
use super::sink::Destination;
use super::{ScalarTypes, integer};
use crate::input::control::Checkpoint;
use crate::{config, errors};
use output::Output;
use pyo3::class::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt};
use sqlaero_core::bind::Context;
use sqlaero_core::json::{Error, Options};
use sqlaero_transcode::diagnostic::{Coordinate, Path};
use sqlaero_transcode::value::{Consumer, ScalarRef};

/// Wrap data as one lazy JSON text parameter.
///
/// data and allow_nan are read-only; mutable contents may change between calls.
/// Every Escape call serializes current contents without caching a document.
/// Keep mutable/provider storage stable during a call, including with NoGIL.
///
/// Serialization is connection-independent UTF-8. Escape then applies the
/// actual MySQL encoding and quoting to that one text value. It is not a SQL
/// fragment or a native MySQL JSON parameter; SQL determines interpretation.
///
/// allow_nan is an exact bool: True maps nonfinite float/Decimal to null;
/// False raises ValueError. Real missing values remain null. Nested JSON
/// wrappers are rejected; pass their data explicitly.
#[pyclass(name = "JSON", module = "sqlaero._sqlaero")]
pub(crate) struct Json {
    data: Option<Py<PyAny>>,
    allow_nan: bool,
}

#[pymethods]
impl Json {
    #[new]
    #[pyo3(signature = (data, *, allow_nan=config::Argument::default()), text_signature = "(data, *, allow_nan=True)")]
    fn new(data: Py<PyAny>, allow_nan: config::Argument<'_>) -> PyResult<Self> {
        Ok(Self {
            data: Some(data),
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

pub(crate) fn option(argument: config::Argument<'_>) -> PyResult<bool> {
    argument
        .0
        .as_ref()
        .map_or(Ok(true), |value| config::boolean(value, "allow_nan"))
}

pub(super) fn failure(py: Python<'_>, error: impl Into<NativeFailure>) -> PyErr {
    let error = match error.into() {
        NativeFailure::Json(error) => error,
        NativeFailure::Mysql(error) => return super::sink::failure(py, error),
    };
    match error {
        Error::Unsupported(name) => errors::builtin_format::<PyTypeError>(
            py,
            format_args!("JSON serialization does not support {name}"),
        ),
        Error::NonFinite | Error::Depth | Error::Range(_) => {
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

struct Serialize<'a, D: Destination + ?Sized = Vec<u8>> {
    types: &'a ScalarTypes,
    options: Options,
    budget: Option<&'a dyn Checkpoint>,
    writer: Output<'a, D>,
    active: Vec<usize>,
    failure_located: bool,
    nodes: usize,
}

impl<D: Destination + ?Sized> Serialize<'_, D> {
    fn locate(&mut self, py: Python<'_>, error: &PyErr, path: &Path<'_>) {
        if !self.failure_located {
            errors::note_format(py, error, format_args!("while serializing JSON at {path}"));
            self.failure_located = true;
        }
    }

    fn value_at(&mut self, value: &Bound<'_, PyAny>, path: &Path<'_>) -> PyResult<()> {
        let result = self.value(value, path);
        if let Err(error) = &result {
            self.locate(value.py(), error, path);
        }
        result
    }

    fn value(&mut self, value: &Bound<'_, PyAny>, path: &Path<'_>) -> PyResult<()> {
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
        if let Some(kind) = super::sequence::kind(value, self.types) {
            return self.container(value, kind, path);
        }
        // Reject active source identity before any provider preparation or
        // required protocol, just as on the common MySQL input boundary.
        if super::transparent::contains(py, &self.active, value.as_ptr() as usize, self.budget)? {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "JSON input contains a true cycle",
            ));
        }
        if self.typed(value, path)? {
            return Ok(());
        }
        if let Some(source) = super::scalar_source::read(self.types, value, self.budget)? {
            use super::scalar_source::Source;
            let native = match source {
                Source::Native(value) => value,
                Source::Datetime(value) => self.types.read_datetime(value, self.budget)?,
                Source::Clock(value) => self.types.read_time(value, self.budget)?,
                Source::Pandas {
                    owner,
                    facts: super::pandas::PandasScalar::Null,
                } => {
                    let _ = owner;
                    ScalarRef::Null
                }
                Source::Pandas {
                    owner,
                    facts:
                        super::pandas::PandasScalar::Temporal {
                            ticks,
                            unit,
                            multiplier,
                            kind,
                            aware,
                        },
                } => {
                    if ticks == i64::MIN {
                        ScalarRef::Null
                    } else if aware {
                        let offset_micros = self
                            .types
                            .read_datetime_offset(owner.cast()?, self.budget)?;
                        ScalarRef::UtcTicks {
                            value: ticks,
                            unit,
                            multiplier,
                            offset_micros,
                        }
                    } else {
                        ScalarRef::Ticks {
                            value: ticks,
                            unit,
                            multiplier,
                            kind,
                        }
                    }
                }
                Source::Numpy(super::numpy::NumpyScalar::Signed(value)) => ScalarRef::Signed(value),
                Source::Numpy(super::numpy::NumpyScalar::Unsigned(value)) => {
                    ScalarRef::Unsigned(value)
                }
                Source::Numpy(super::numpy::NumpyScalar::Float(value)) => ScalarRef::Float(value),
                Source::Numpy(super::numpy::NumpyScalar::Bool(value)) => ScalarRef::Boolean(value),
                Source::Numpy(super::numpy::NumpyScalar::Temporal {
                    ticks,
                    unit,
                    multiplier,
                    kind,
                }) => {
                    if ticks == i64::MIN {
                        ScalarRef::Null
                    } else {
                        ScalarRef::Ticks {
                            value: ticks,
                            unit,
                            multiplier,
                            kind,
                        }
                    }
                }
                Source::Decimal(value) => {
                    return self
                        .types
                        .decimal
                        .read(value, self.budget, |header, digits| {
                            super::decimal::consume(
                                py,
                                header,
                                digits,
                                &mut self.writer,
                                &self.options,
                                self.budget,
                                |error| failure(py, error),
                            )
                        });
                }
                Source::StructTime(value) => return self.struct_time(value, path),
                Source::Json(_) => {
                    return Err(errors::builtin_message::<PyTypeError>(
                        py,
                        "nested JSON wrappers are not supported",
                    ));
                }
            };
            return self
                .writer
                .write(native, &self.options)
                .map_err(|error| failure(py, error));
        }
        if let Some(source) = self.types.transparent(value, self.budget)? {
            if source.intersects(&self.active, self.budget)? {
                return Err(errors::builtin_message::<PyValueError>(
                    py,
                    "transparent JSON source contains a true cycle",
                ));
            }
            let previous = self.active.len();
            self.active
                .try_reserve(source.len())
                .map_err(|_| errors::memory_error(py))?;
            if let Err(error) =
                source.for_each_identity(self.budget, |identity| self.active.push(identity))
            {
                self.active.truncate(previous);
                return Err(error);
            }
            let result = self.value_at(source.payload(), path);
            self.active.truncate(previous);
            return result;
        }
        let detail = self.types.diagnostic(value)?;
        Err(errors::builtin_format::<PyTypeError>(
            py,
            format_args!("unsupported JSON input: {detail}"),
        ))
    }

    fn container(&mut self, value: &Bound<'_, PyAny>, kind: Kind, path: &Path<'_>) -> PyResult<()> {
        let py = value.py();
        let identity = value.as_ptr() as usize;
        if super::transparent::contains(py, &self.active, identity, self.budget)? {
            return Err(errors::builtin_message::<PyValueError>(
                py,
                "JSON input contains a true cycle",
            ));
        }
        self.active
            .try_reserve(1)
            .map_err(|_| errors::memory_error(py))?;
        self.active.push(identity);
        let record = if kind == Kind::Tuple {
            super::record::names(value, self.budget)?
        } else {
            None
        };
        let object = kind == Kind::Dict || record.is_some();
        if object {
            self.writer.object_start()
        } else {
            self.writer.array_start()
        }
        .map_err(|error| failure(py, error))?;
        let projection = if object || kind == Kind::Items {
            Projection::Entries
        } else {
            Projection::Values
        };
        let sequence = self
            .types
            .sequences
            .prepare(value, kind, projection, self.budget)?;
        let mut names: HashMap<u64, Vec<String>> = HashMap::new();
        let seed = RandomState::new();
        for index in 0..sequence.len() {
            let entry = path.child(if object {
                Coordinate::Key(index)
            } else {
                Coordinate::Index(index)
            });
            let result = (|| {
                if index.is_multiple_of(256) {
                    super::checkpoint(py, self.budget)?;
                }
                let current = sequence.row_item(index, self.budget)?;
                if object {
                    let (source_key, source_value) = if let Some(record) = &record {
                        (record.get(index)?, current)
                    } else {
                        let pair = current.cast::<pyo3::types::PyTuple>()?;
                        (pair.get_item(0)?, pair.get_item(1)?)
                    };
                    let key = keys::read(&source_key, self.types, self.budget, &seed)?;
                    names.try_reserve(1).map_err(|_| errors::memory_error(py))?;
                    if let Some(previous) = names.get(&key.hash) {
                        for name in previous {
                            if keys::same(py, name, &key.value, self.budget)? {
                                return Err(errors::builtin_format::<PyValueError>(
                                    py,
                                    format_args!(
                                        "JSON object keys collide after normalization at entry {index}"
                                    ),
                                ));
                            }
                        }
                    }
                    self.writer
                        .key_start(key.value.len())
                        .map_err(|error| failure(py, error))?;
                    super::unicode::native_fragments(py, &key.value, self.budget, |_, text| {
                        self.writer
                            .text_fragment(text, &self.options)
                            .map_err(|error| failure(py, error))
                    })?;
                    self.writer
                        .text_end(&self.options)
                        .map_err(|error| failure(py, error))?;
                    let child = path.child(Coordinate::Field {
                        name: &key.value,
                        ordinal: index,
                    });
                    self.value_at(&source_value, &child)?;
                    let previous = names.entry(key.hash).or_default();
                    previous
                        .try_reserve(1)
                        .map_err(|_| errors::memory_error(py))?;
                    previous.push(key.value);
                } else {
                    self.value_at(&current, &entry)?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.locate(py, &error, &entry);
                return Err(error);
            }
        }
        if object {
            self.writer.object_end()
        } else {
            self.writer.array_end()
        }
        .map_err(|error| failure(py, error))?;
        self.active.pop();
        Ok(())
    }

    fn integer(&mut self, source: &Bound<'_, PyInt>) -> PyResult<()> {
        let py = source.py();
        let integer = integer::Decimal::acquire(source, self.budget)?;
        self.writer
            .integer_start(integer.negative)
            .map_err(|error| failure(py, error))?;
        integer.fragments(py, self.budget, |text| {
            self.writer
                .integer_fragment(text.as_bytes())
                .map_err(|error| failure(py, error))
        })?;
        self.writer
            .integer_end()
            .map_err(|error| failure(py, error))?;
        super::checkpoint(py, self.budget)
    }

    fn struct_time(
        &mut self,
        source: &Bound<'_, pyo3::types::PyTuple>,
        path: &Path<'_>,
    ) -> PyResult<()> {
        let py = source.py();
        let identity = source.as_ptr() as usize;
        if super::transparent::contains(py, &self.active, identity, self.budget)? {
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
            .object_start()
            .map_err(|error| failure(py, error))?;
        for (index, name) in super::standard::STRUCT_FIELDS.iter().enumerate() {
            self.writer
                .key_start(name.len())
                .map_err(|error| failure(py, error))?;
            self.writer
                .text_fragment(name, &self.options)
                .map_err(|error| failure(py, error))?;
            self.writer
                .text_end(&self.options)
                .map_err(|error| failure(py, error))?;
            let child = path.child(Coordinate::Field {
                name,
                ordinal: index,
            });
            let value = super::standard::struct_field(source, index)?;
            self.value_at(&value, &child)?;
        }
        self.writer
            .object_end()
            .map_err(|error| failure(py, error))?;
        self.active.pop();
        Ok(())
    }
}

pub(crate) fn serialize(
    value: &Bound<'_, PyAny>,
    types: &ScalarTypes,
    allow_nan: bool,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<String> {
    let local = super::control::Control::unlimited();
    let budget = budget.unwrap_or(&local as &dyn Checkpoint);
    serialize_into(
        value,
        types,
        allow_nan,
        budget,
        Output::<Vec<u8>>::buffered(),
    )
}

fn serialize_into<'a, D: Destination + ?Sized>(
    value: &Bound<'_, PyAny>,
    types: &'a ScalarTypes,
    allow_nan: bool,
    budget: &'a dyn Checkpoint,
    writer: Output<'a, D>,
) -> PyResult<String> {
    let py = value.py();
    let budget = Some(budget);
    super::checkpoint(py, budget)?;
    let mut state = Serialize {
        types,
        options: Options { allow_nan },
        budget,
        writer,
        active: Vec::new(),
        failure_located: false,
        nodes: 0,
    };
    let root = Path::root("data");
    state.value_at(value, &root)?;
    if let Err(error) = super::checkpoint(py, budget) {
        state.locate(py, &error, &root);
        return Err(error);
    }
    state.writer.finish().map_err(|error| {
        let error = failure(py, error);
        errors::note_format(py, &error, format_args!("while serializing JSON at {root}"));
        error
    })
}

/// Serialize one lazy wrapper as text, never as source SQL or binary input.
pub(crate) fn mysql<D: Destination + ?Sized>(
    value: &Bound<'_, Json>,
    types: &ScalarTypes,
    output: &mut D,
    context: &Context,
    budget: Option<&dyn Checkpoint>,
) -> PyResult<()> {
    let py = value.py();
    let (data, allow_nan) = {
        let value = value.try_borrow()?;
        (value.data(py), value.allow_nan)
    };
    if context.codec.name() == "utf8mb4" && output.can_stream_text() {
        let local = super::control::Control::unlimited();
        let budget = budget.unwrap_or(&local as &dyn Checkpoint);
        output
            .text_start(None, context)
            .map_err(|error| super::sink::failure(py, error))?;
        serialize_into(
            data.bind(py),
            types,
            allow_nan,
            budget,
            Output::streaming(output, context),
        )?;
        super::checkpoint(py, Some(budget))?;
        return output
            .text_end(context)
            .map_err(|error| super::sink::failure(py, error));
    }
    let text = serialize(data.bind(py), types, allow_nan, budget)?;
    // A native String is valid UTF-8 by construction. No whole-document scan
    // or Python JSON str/bytes is introduced before the target encodes it.
    super::text_utf8(py, &text, output, context, budget)
}

/// Return one complete compact UTF-8 JSON document as built-in bytes.
///
/// No Connection, SQL mode, SQL timeout or MySQL packet quota is consulted.
/// allow_nan is a keyword-only exact bool: True maps nonfinite float/Decimal
/// to null; False raises ValueError. Real missing values remain null. Pass
/// wrapped.data rather than JSON wrappers. Subclasses use actual base storage.
///
/// Decimal uses representation-preserving strings; temporal precision and
/// array/object/table structure follow the documented target policy.
/// Unsupported types raise TypeError; cycles, excessive output nesting and
/// colliding keys raise ValueError; actual surrogate codepoints raise
/// UnicodeEncodeError. Original source/signal exceptions propagate unchanged.
/// No partial document is returned.
///
/// Keep mutable/provider storage stable. Native work checks signals
/// cooperatively; necessary public source/ownership calls can be atomic.
/// This does not promise an instant cancellation delay.
#[pyfunction]
#[pyo3(pass_module, signature = (data, *, allow_nan=config::Argument::default()), text_signature = "($module, data, *, allow_nan=True)")]
pub(crate) fn json_dumps<'py>(
    module: &Bound<'py, PyModule>,
    data: &Bound<'py, PyAny>,
    allow_nan: config::Argument<'py>,
) -> PyResult<Bound<'py, PyBytes>> {
    let py = module.py();
    let owner = module.getattr(c"_scalar_types")?;
    let types = owner.extract::<PyRef<'_, ScalarTypes>>()?;
    let text = serialize(data, &types, option(allow_nan)?, None)?;
    Ok(crate::output::bytes_bounded(py, text.as_bytes(), None)?.cast_into::<PyBytes>()?)
}
