//! Canonical CPU Arrow sources, imported through the public C Data interface.

use crate::input::control::Checkpoint;
use std::ffi::{c_char, c_int, c_void};
use std::ptr;

use super::cell::{NativeFailure, Schema, Target};
use super::sink::Destination;
use arrow_array::ffi::FFI_ArrowArray;
use arrow_array::types::{
    Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use arrow_array::{
    Array, BooleanArray, Date32Array, Date64Array, Decimal32Array, Decimal64Array, Decimal128Array,
    Decimal256Array, DurationMicrosecondArray, DurationMillisecondArray, DurationNanosecondArray,
    DurationSecondArray, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
    Time32MillisecondArray, Time32SecondArray, Time64MicrosecondArray, Time64NanosecondArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow_buffer::{ArrowNativeType, BooleanBuffer, NullBuffer, ScalarBuffer};
use arrow_data::{ByteView, validate_binary_view};
use arrow_schema::{DataType, TimeUnit as ArrowTimeUnit, ffi::FFI_ArrowSchema};
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple, PyType};
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{ValueError, ValueRef};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::error::ClientErrorCode;

use super::source::{CellWriter, PreparedSource, Shape};
use crate::errors;
use sqlaero_transcode::temporal::{TimeKind, TimeUnit};

mod dictionary;
pub(super) mod names;
mod native;
pub(super) use native::ArrayRef;
use native::Record as StructArray;
mod offset;
mod projection;
mod schema;
use projection::consume as consume_value;
use schema::{Field, View as TypeView};

/// The five fields of the public Arrow C Stream ABI. Apache's record-batch
/// reader requires a Struct root, while this input also accepts primitive
/// streams from ChunkedArray and Polars Series. Only the pull wrapper lives
/// here; array/schema parsing and native buffers use Apache Arrow.
#[repr(C)]
struct StreamAbi {
    get_schema: Option<unsafe extern "C" fn(*mut Self, *mut FFI_ArrowSchema) -> c_int>,
    get_next: Option<unsafe extern "C" fn(*mut Self, *mut FFI_ArrowArray) -> c_int>,
    get_last_error: Option<unsafe extern "C" fn(*mut Self) -> *const c_char>,
    release: Option<unsafe extern "C" fn(*mut Self)>,
    private_data: *mut c_void,
}

impl StreamAbi {
    fn empty() -> Self {
        Self {
            get_schema: None,
            get_next: None,
            get_last_error: None,
            release: None,
            private_data: ptr::null_mut(),
        }
    }

    fn failure(&mut self, py: Python<'_>, operation: &str, status: c_int) -> PyErr {
        // A producer may have raised an actual Python exception while handling
        // the public export/callback. Preserve it before optional diagnostics.
        // SAFETY: the source is used on this attached Python thread.
        if !unsafe { pyo3::ffi::PyErr_Occurred() }.is_null() {
            return PyErr::fetch(py);
        }
        let mut bytes = [0u8; 512];
        let mut length = 0;
        let mut clipped = false;
        if let Some(callback) = self.get_last_error {
            // SAFETY: called only after a failed operation on a live standard
            // stream. The producer owns the returned NUL-terminated message.
            let message = unsafe { callback(self) };
            if !message.is_null() {
                while length < bytes.len() {
                    // SAFETY: a valid C string is readable through its NUL;
                    // stop at that NUL, or after the bounded diagnostic prefix.
                    let byte = unsafe { *message.add(length).cast::<u8>() };
                    if byte == 0 {
                        break;
                    }
                    bytes[length] = byte;
                    length += 1;
                }
                clipped = length == bytes.len();
            }
        }
        errors::builtin_format::<PyTypeError>(
            py,
            format_args!(
                "Arrow stream {operation} failed with status {status}: {}{}",
                bytes[..length].escape_ascii(),
                if clipped { " [truncated]" } else { "" }
            ),
        )
    }
}

impl Drop for StreamAbi {
    fn drop(&mut self) {
        if let Some(release) = self.release {
            // SAFETY: this is the unique moved C stream owner. It is held only
            // in an attached boundary source; no Send/Sync is implemented.
            unsafe { release(self) };
        }
    }
}

/// Optional random-label history keeps ordinary inline source headers small.
struct RetainedChunks {
    chunks: Vec<(usize, ArrayRef)>,
}

enum Storage {
    Single(ArrayRef),
    Stream {
        stream: StreamAbi,
        current: Option<ArrayRef>,
        start: usize,
        end: usize,
        retained: Option<Box<RetainedChunks>>,
    },
}

/// Boundary-owned source. Rust Arrow buffers own the moved C release callback;
/// the Python owner and token keep provider access/release in the right domain.
pub(crate) struct Prepared<'py> {
    storage: Storage,
    fast_path: Option<Box<FastPath>>,
    field: Field,
    shape: Shape,
    owner: Bound<'py, PyAny>,
    offsets: Option<Box<offset::Offsets<'py>>>,
    names: names::Source<'py>,
}

/// Attached source metadata and its current schema position. This carries no
/// SQL context or formatted answers, and is borrowed only for this traversal.
struct Read<'a, 'py> {
    owner: &'a Bound<'py, PyAny>,
    offsets: &'a mut Option<Box<offset::Offsets<'py>>>,
    names: &'a mut names::Source<'py>,
    node: usize,
}

impl<'py> Read<'_, 'py> {
    fn at(&mut self, node: usize) -> Read<'_, 'py> {
        Read {
            owner: self.owner,
            offsets: self.offsets,
            names: self.names,
            node,
        }
    }
    fn child(
        &mut self,
        edge: names::Edge,
        dtype: TypeView<'_>,
        wanted: bool,
        budget: &dyn Checkpoint,
    ) -> PyResult<usize> {
        if wanted && names::needed(dtype) {
            self.names.child(self.node, edge, budget)
        } else {
            Ok(0)
        }
    }
    fn dictionary(
        &mut self,
        mut dtype: TypeView<'_>,
        wanted: bool,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        while let TypeView::Dictionary(_, value) = dtype {
            self.node = self.child(names::Edge::Value, value.data_type(), wanted, budget)?;
            dtype = value.data_type();
        }
        Ok(())
    }
}

/// These strategies are mutually exclusive for one validated source schema.
/// A generic source owns neither, keeping the original Prepared header small.
enum FastPath {
    Dictionary(dictionary::Reader),
    Int64(Int64Views),
    Float32(FloatReader<f32>),
    Boolean(BooleanReader),
    Float64(FloatReader<f64>),
}

/// Public typed arrays retain their own buffers and validity/offset metadata.
/// This replaces repeated dynamic downcasts, not the original root ownership
/// or the producer's required buffer-lifetime and input-stability contract.
enum Int64Views {
    Vector(Int64Array),
    Table(Vec<Int64Array>),
}

impl Int64Views {
    fn prepare(
        py: Python<'_>,
        array: &ArrayRef,
        shape: Shape,
        budget: &dyn Checkpoint,
    ) -> PyResult<Option<Self>> {
        // Tiny chunks cannot amortize new strategy/column allocations. This
        // also keeps long sources made of many tiny chunks on the old path.
        if array.len() < 32 {
            return Ok(None);
        }
        let views = match shape {
            Shape::Vector { .. } if array.data_type().is_scalar(&DataType::Int64) => {
                let values = array
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .ok_or_else(|| invalid(py, "Arrow logical and physical array types differ"))?;
                Self::Vector(values.clone())
            }
            Shape::Table { columns, .. } if columns != 0 => {
                let TypeView::Struct(fields) = array.data_type() else {
                    return Ok(None);
                };
                // Nullable Struct parents and nested Struct vectors retain
                // their existing visitor. No parent validity is discarded.
                if fields.len() != columns || array.null_count() != 0 {
                    return Ok(None);
                }
                for (index, field) in fields.iter().enumerate() {
                    if index != 0 && index.is_multiple_of(64) {
                        super::check(py, budget)?;
                    }
                    if !field.data_type().is_scalar(&DataType::Int64) {
                        return Ok(None);
                    }
                }
                let data = array
                    .as_any()
                    .downcast_ref::<StructArray>()
                    .ok_or_else(|| invalid(py, "Arrow table did not produce a Struct array"))?;
                let mut values = Vec::new();
                values
                    .try_reserve_exact(columns)
                    .map_err(|_| errors::memory_error(py))?;
                for column in 0..columns {
                    if column != 0 && column.is_multiple_of(64) {
                        super::check(py, budget)?;
                    }
                    let value = data
                        .column(column)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .ok_or_else(|| {
                            invalid(py, "Arrow logical and physical array types differ")
                        })?;
                    values.push(value.clone());
                }
                Self::Table(values)
            }
            _ => return Ok(None),
        };
        Ok(Some(views))
    }

    fn read(&self, py: Python<'_>, row: usize, column: usize) -> PyResult<ValueRef<'static>> {
        let values = match self {
            Self::Vector(values) => values,
            Self::Table(columns) => columns
                .get(column)
                .ok_or_else(|| invalid(py, "Arrow source column is outside its prepared schema"))?,
        };
        Self::read_value(py, values, row)
    }

    fn read_value(py: Python<'_>, values: &Int64Array, row: usize) -> PyResult<ValueRef<'static>> {
        if row >= values.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        Ok(if values.is_null(row) {
            ValueRef::Null
        } else {
            ValueRef::Signed(values.value(row))
        })
    }
}

/// Own only the two public buffers used by the native float vector readers.
/// Logical slices already contain source offsets; never apply them twice.
/// The two primitive instantiations convert to binary64 statically.
struct FloatReader<T: ArrowNativeType + Into<f64>> {
    values: ScalarBuffer<T>,
    nulls: Option<NullBuffer>,
}

impl<T: ArrowNativeType + Into<f64>> FloatReader<T> {
    fn read(&self, py: Python<'_>, row: usize) -> PyResult<ValueRef<'static>> {
        if row >= self.values.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        Ok(
            if self.nulls.as_ref().is_some_and(|nulls| nulls.is_null(row)) {
                ValueRef::Null
            } else {
                ValueRef::Float(self.values[row].into())
            },
        )
    }
}

/// Retain only the public bit-packed data and validity used by this reader.
/// Each buffer already carries its own bit offset and logical slice length.
struct BooleanReader {
    values: BooleanBuffer,
    nulls: Option<NullBuffer>,
}

impl BooleanReader {
    fn prepare(array: &ArrayRef) -> Option<Self> {
        if !array.data_type().is_scalar(&DataType::Boolean) {
            return None;
        }
        // A future physical mismatch keeps the generic value's late failure.
        let values = array.as_any().downcast_ref::<BooleanArray>()?;
        Some(Self {
            values: values.values().clone(),
            nulls: values.nulls().cloned(),
        })
    }

    fn read(&self, py: Python<'_>, row: usize) -> PyResult<ValueRef<'static>> {
        if row >= self.values.len() {
            return Err(invalid(py, "Arrow child index is outside its array"));
        }
        Ok(
            if self.nulls.as_ref().is_some_and(|nulls| nulls.is_null(row)) {
                ValueRef::Null
            } else {
                ValueRef::Boolean(self.values.value(row))
            },
        )
    }
}

/// Keep existing native strategies first. Only long primitive vectors prepare
/// an additional data/validity owner, after the current chunk is valid.
fn native_fast_path(
    py: Python<'_>,
    array: &ArrayRef,
    shape: Shape,
    budget: &dyn Checkpoint,
) -> PyResult<Option<FastPath>> {
    if let Some(reader) = dictionary::Reader::prepare(array.data_type(), shape) {
        return Ok(Some(FastPath::Dictionary(reader)));
    }
    if let Some(views) = Int64Views::prepare(py, array, shape, budget)? {
        return Ok(Some(FastPath::Int64(views)));
    }
    if !matches!(shape, Shape::Vector { length } if length >= 32) || array.len() < 32 {
        return Ok(None);
    }
    if !array.data_type().is_scalar(&DataType::Float32) {
        if let Some(reader) = BooleanReader::prepare(array) {
            return Ok(Some(FastPath::Boolean(reader)));
        }
        if array.data_type().is_scalar(&DataType::Float64) {
            // Keep any future physical mismatch on the generic value path;
            // an unused or NULL value must not acquire an earlier type error.
            let Some(values) = array.as_any().downcast_ref::<Float64Array>() else {
                return Ok(None);
            };
            return Ok(Some(FastPath::Float64(FloatReader {
                values: values.values().clone(),
                nulls: values.nulls().cloned(),
            })));
        }
        return Ok(None);
    }
    let values = array
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| invalid(py, "Arrow logical and physical array types differ"))?;
    Ok(Some(FastPath::Float32(FloatReader {
        values: values.values().clone(),
        nulls: values.nulls().cloned(),
    })))
}

fn invalid(py: Python<'_>, message: impl AsRef<str>) -> PyErr {
    errors::builtin_message::<PyTypeError>(py, message)
}

fn current_python_error(py: Python<'_>) -> PyResult<()> {
    // SAFETY: this thread is attached; observing the current error indicator
    // does not call Python or normalize/replace the original exception.
    if unsafe { pyo3::ffi::PyErr_Occurred() }.is_null() {
        Ok(())
    } else {
        Err(PyErr::fetch(py))
    }
}

fn loaded_module<'py>(py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
    // SAFETY: CPython owns sys.modules and returns a borrowed, live dictionary;
    // the Bound acquires its own reference while this thread is attached.
    let modules = unsafe { Bound::from_borrowed_ptr(py, pyo3::ffi::PyImport_GetModuleDict()) }
        .cast_into::<PyDict>()?;
    let key = PyString::from_bytes(py, b"pyarrow")?;
    let Some(module) = modules.get_item(key)? else {
        return Ok(None);
    };
    if module.is_none() {
        return Ok(None);
    }
    Ok(Some(module))
}

/// Use the actual type hierarchy and canonical provider identity in both the
/// direct Array input and the NumPy-allocation-owner bridge.
fn canonical_array(value: &Bound<'_, PyAny>, module: &Bound<'_, PyAny>) -> PyResult<bool> {
    let array_type = errors::attribute(module, c"Array")?.cast_into::<PyType>()?;
    if !super::actual_instance(value, &array_type) {
        return Ok(false);
    }
    let dtype = value.get_type();
    let name = dtype.name()?;
    let library = errors::attribute(module, c"lib")?;
    let Some(canonical) = library.getattr_opt(&name)? else {
        return Ok(false);
    };
    if !dtype.is(&canonical) {
        return Ok(false);
    }
    errors::attribute(value, c"is_cpu")?.extract()
}

/// Retain a genuine CPU Buffer containing a NumPy view's origin address.
/// Array.buffers() supplies physical storage, including a slice's parent
/// allocation; neither logical Array bytes nor NumPy nbytes define this range.
/// Only metadata is inspected here. The NumPy reader acquires its own public
/// buffer export before checking the complete proposed shape/stride span.
pub(crate) fn allocation_buffer<'py>(
    owner: &Bound<'py, PyAny>,
    address: usize,
    budget: &dyn Checkpoint,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let py = owner.py();
    super::check(py, budget)?;
    let Some(module) = loaded_module(py)? else {
        return Ok(None);
    };
    if !canonical_array(owner, &module)? {
        return Ok(None);
    }
    let buffer_type = errors::attribute(&module, c"Buffer")?.cast_into::<PyType>()?;
    let buffers = owner.call_method0(c"buffers")?.cast_into::<PyList>()?;
    super::check(py, budget)?;
    for index in 0..buffers.len() {
        super::check(py, budget)?;
        let buffer = buffers.get_item(index)?;
        if buffer.is_none() {
            continue;
        }
        if !super::actual_instance(&buffer, &buffer_type) {
            return Err(invalid(py, "Arrow buffers() returned a non-Buffer object"));
        }
        if !errors::attribute(&buffer, c"is_cpu")?.extract::<bool>()? {
            return Ok(None);
        }
        let start: usize = errors::attribute(&buffer, c"address")?.extract()?;
        let length: usize = errors::attribute(&buffer, c"size")?.extract()?;
        if length == 0 {
            continue;
        }
        let end = start.checked_add(length).ok_or_else(|| {
            errors::builtin_message::<PyBufferError>(
                py,
                "Arrow Buffer address range overflows usize",
            )
        })?;
        if start == 0 {
            return Err(errors::builtin_message::<PyBufferError>(
                py,
                "nonempty Arrow Buffer has a null address",
            ));
        }
        if start <= address && address < end {
            return Ok(Some(buffer));
        }
    }
    Ok(None)
}

/// Recognize only canonical PyArrow CPU classes already loaded by the caller.
/// Ordinary SQLAero input never imports an optional provider merely to probe it.
pub(crate) fn prepare<'py>(
    value: &Bound<'py, PyAny>,
    budget: &dyn Checkpoint,
) -> PyResult<Option<Prepared<'py>>> {
    let py = value.py();
    super::check(py, budget)?;
    let Some(module) = loaded_module(py)? else {
        return Ok(None);
    };
    let dtype = value.get_type();
    let shape = if canonical_array(value, &module)?
        || dtype.is(&errors::attribute(&module, c"ChunkedArray")?)
    {
        Shape::Vector {
            length: value.len()?,
        }
    } else if dtype.is(&errors::attribute(&module, c"RecordBatch")?)
        || dtype.is(&errors::attribute(&module, c"Table")?)
    {
        Shape::Table {
            rows: errors::attribute(value, c"num_rows")?.extract()?,
            columns: errors::attribute(value, c"num_columns")?.extract()?,
        }
    } else {
        return Ok(None);
    };
    prepare_exported(
        value,
        value,
        shape,
        names::Source::arrow(value, shape),
        budget,
    )
    .map(Some)
}

/// Import a public CPU export whose canonical outer type was already checked.
/// Polars calls this after separating Object columns and preserving its shape.
pub(crate) fn prepare_exported<'py>(
    owner: &Bound<'py, PyAny>,
    exported: &Bound<'py, PyAny>,
    shape: Shape,
    mut names: names::Source<'py>,
    budget: &dyn Checkpoint,
) -> PyResult<Prepared<'py>> {
    let py = owner.py();
    super::check(py, budget)?;
    if exported.hasattr(c"__arrow_c_array__")? {
        let capsules = exported.call_method0(c"__arrow_c_array__")?;
        super::check(py, budget)?;
        let pair = capsules.cast::<PyTuple>()?;
        if pair.len() != 2 {
            return Err(invalid(
                py,
                "Arrow array export must return schema and array capsules",
            ));
        }
        let schema_capsule = pair.get_item(0)?;
        let array_capsule = pair.get_item(1)?;
        // SAFETY: named PyCapsules are checked by CPython. Both complete C
        // structures are moved once, leaving empty release slots in capsules.
        let (schema, array) = unsafe {
            let schema =
                pyo3::ffi::PyCapsule_GetPointer(schema_capsule.as_ptr(), c"arrow_schema".as_ptr())
                    .cast::<FFI_ArrowSchema>();
            if schema.is_null() {
                return Err(PyErr::fetch(py));
            }
            let array =
                pyo3::ffi::PyCapsule_GetPointer(array_capsule.as_ptr(), c"arrow_array".as_ptr())
                    .cast::<FFI_ArrowArray>();
            if array.is_null() {
                return Err(PyErr::fetch(py));
            }
            if (*schema).release().is_none() || (*array).is_released() {
                return Err(invalid(
                    py,
                    "Arrow schema or array capsule was already consumed",
                ));
            }
            (
                FFI_ArrowSchema::from_raw(schema),
                FFI_ArrowArray::from_raw(array),
            )
        };
        let field = schema::import(py, &schema, &mut names, budget)?;
        validate_field(py, &field, 0, budget)?;
        super::check(py, budget)?;
        let array = native::import(py, array, &field, budget)?;
        validate_root_shape(py, &array, shape)?;
        let fast_path = native_fast_path(py, &array, shape, budget)?
            .map(|views| super::boxed(py, views))
            .transpose()?;
        return Ok(Prepared {
            storage: Storage::Single(array),
            fast_path,
            field,
            shape,
            owner: owner.clone(),
            offsets: None,
            names,
        });
    }
    let capsule = exported.call_method0(c"__arrow_c_stream__")?;
    super::check(py, budget)?;
    // SAFETY: CPython checks the standard CPU capsule name. The C ABI is
    // specified by Arrow, not inferred from a private Python object layout.
    let mut stream = unsafe {
        let raw = pyo3::ffi::PyCapsule_GetPointer(capsule.as_ptr(), c"arrow_array_stream".as_ptr())
            .cast::<StreamAbi>();
        if raw.is_null() {
            return Err(PyErr::fetch(py));
        }
        if (*raw).release.is_none() {
            return Err(invalid(py, "Arrow stream capsule was already consumed"));
        }
        ptr::replace(raw, StreamAbi::empty())
    };
    let Some(get_schema) = stream.get_schema else {
        return Err(invalid(py, "Arrow stream has no get_schema callback"));
    };
    if stream.get_next.is_none() {
        return Err(invalid(py, "Arrow stream has no get_next callback"));
    }
    let mut schema = FFI_ArrowSchema::empty();
    // SAFETY: the live standard stream owns both callback and private state;
    // schema is an initialized empty out-parameter with its own release guard.
    let status = unsafe { get_schema(&mut stream, &mut schema) };
    current_python_error(py)?;
    if status != 0 {
        return Err(stream.failure(py, "get_schema", status));
    }
    super::check(py, budget)?;
    if schema.release().is_none() {
        return Err(invalid(py, "Arrow stream returned a released schema"));
    }
    let field = schema::import(py, &schema, &mut names, budget)?;
    validate_field(py, &field, 0, budget)?;
    validate_table_schema(py, &field, shape)?;
    super::check(py, budget)?;
    Ok(Prepared {
        storage: Storage::Stream {
            stream,
            current: None,
            start: 0,
            end: 0,
            retained: None,
        },
        fast_path: None,
        field,
        shape,
        owner: owner.clone(),
        offsets: None,
        names,
    })
}

fn validate_table_schema(py: Python<'_>, field: &Field, shape: Shape) -> PyResult<()> {
    if let Shape::Table { columns, .. } = shape {
        match field.data_type() {
            TypeView::Struct(fields) if fields.len() == columns => {}
            _ => {
                return Err(invalid(
                    py,
                    "Arrow table schema does not match its declared columns",
                ));
            }
        }
    }
    Ok(())
}

fn validate_root_shape(py: Python<'_>, array: &ArrayRef, shape: Shape) -> PyResult<()> {
    match shape {
        Shape::Vector { length } if array.len() == length => Ok(()),
        Shape::Table { rows, columns }
            if array.len() == rows
                && matches!(array.data_type(), TypeView::Struct(fields) if fields.len() == columns) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            py,
            "Arrow array does not match its prepared logical shape",
        )),
    }
}

fn validate_field(
    py: Python<'_>,
    field: &Field,
    depth: usize,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    budget.bytes(py, 8)?;
    if depth >= 64 {
        return Err(errors::client(
            py,
            ClientErrorCode::ContainerNesting,
            "Arrow schema exceeds 64 nested container levels",
        ));
    }
    match field.data_type() {
        TypeView::Scalar(dtype) => match dtype {
            DataType::Null
            | DataType::Boolean
            | DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
            | DataType::Utf8
            | DataType::LargeUtf8
            | DataType::Utf8View
            | DataType::Binary
            | DataType::LargeBinary
            | DataType::BinaryView
            | DataType::Date32
            | DataType::Date64
            | DataType::Timestamp(_, _)
            | DataType::Duration(_)
            | DataType::Decimal32(_, _)
            | DataType::Decimal64(_, _)
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _) => Ok(()),
            DataType::Time32(ArrowTimeUnit::Second | ArrowTimeUnit::Millisecond)
            | DataType::Time64(ArrowTimeUnit::Microsecond | ArrowTimeUnit::Nanosecond) => Ok(()),
            DataType::FixedSizeBinary(width) if *width >= 0 => Ok(()),
            _ => Err(invalid(py, "unsupported Arrow logical input dtype")),
        },
        TypeView::List(child) | TypeView::LargeList(child) => {
            validate_field(py, child, depth + 1, budget)
        }
        TypeView::FixedSizeList(child, width) if *width >= 0 => {
            validate_field(py, child, depth + 1, budget)
        }
        TypeView::Struct(fields) => {
            for child in fields {
                validate_field(py, child, depth + 1, budget)?;
            }
            Ok(())
        }
        TypeView::Dictionary(key, value) => {
            if !matches!(
                key,
                DataType::Int8
                    | DataType::Int16
                    | DataType::Int32
                    | DataType::Int64
                    | DataType::UInt8
                    | DataType::UInt16
                    | DataType::UInt32
                    | DataType::UInt64
            ) {
                return Err(invalid(
                    py,
                    "Arrow dictionary key must be an 8/16/32/64-bit integer",
                ));
            }
            validate_field(py, value, depth + 1, budget)
        }
        _ => Err(invalid(py, "unsupported Arrow logical input dtype")),
    }
}

impl Prepared<'_> {
    /// Qualified categorical labels need native random access. Retain only
    /// already visited chunk owners, never values, Python rows or SQL bytes.
    /// Ordinary streaming inputs keep their one-current-chunk release policy.
    pub(crate) fn enable_random(&mut self, budget: &dyn Checkpoint) -> PyResult<()> {
        let py = self.owner.py();
        super::check(py, budget)?;
        if let Storage::Stream { retained, .. } = &mut self.storage
            && retained.is_none()
        {
            *retained = Some(super::boxed(py, RetainedChunks { chunks: Vec::new() })?);
        }
        Ok(())
    }
}

impl Storage {
    /// Borrow the root already owned by the single array, current stream
    /// chunk or retained label history. Its owner stays alive through this
    /// projection; individual provider byte borrows keep their own checks.
    fn current_array(
        &mut self,
        py: Python<'_>,
        row: usize,
        field: &Field,
        shape: Shape,
        fast_path: &mut Option<Box<FastPath>>,
        budget: &dyn Checkpoint,
    ) -> PyResult<(&ArrayRef, usize)> {
        match self {
            Storage::Single(array) => Ok((array, row)),
            Storage::Stream {
                stream,
                current,
                start,
                end,
                retained,
            } => {
                if row < *start {
                    if let Some(retained) = retained {
                        // Numeric binary search takes at most pointer-width
                        // steps and never scans labels or exported values.
                        let after = retained.chunks.partition_point(|(start, _)| *start <= row);
                        if let Some((begin, array)) = after
                            .checked_sub(1)
                            .and_then(|index| retained.chunks.get(index))
                            && row - *begin < array.len()
                        {
                            *fast_path = None;
                            return Ok((array, row - *begin));
                        }
                    }
                    return Err(invalid(py, "Arrow stream cannot replay an earlier chunk"));
                }
                while current.is_none() || row >= *end {
                    super::check(py, budget)?;
                    let Some(next) = stream.get_next else {
                        return Err(invalid(
                            py,
                            "Arrow stream no longer has a get_next callback",
                        ));
                    };
                    let mut raw = FFI_ArrowArray::empty();
                    // SAFETY: the stream is live and uniquely held by this
                    // attached source, and raw is a guarded empty out-value.
                    let status = unsafe { next(stream, &mut raw) };
                    current_python_error(py)?;
                    if status != 0 {
                        return Err(stream.failure(py, "get_next", status));
                    }
                    super::check(py, budget)?;
                    if raw.is_released() {
                        return Err(invalid(
                            py,
                            "Arrow stream ended before its declared row count",
                        ));
                    }
                    let array = native::import(py, raw, field, budget)?;
                    let next_end = end
                        .checked_add(array.len())
                        .ok_or_else(|| invalid(py, "Arrow stream length overflows usize"))?;
                    let next_fast_path =
                        if matches!(fast_path.as_deref(), Some(FastPath::Dictionary(_))) {
                            None
                        } else {
                            native_fast_path(py, &array, shape, budget)?
                                .map(|views| super::boxed(py, views))
                                .transpose()?
                        };
                    if let Some(retained) = retained
                        && let Some(previous) = current.as_ref()
                        && !previous.is_empty()
                    {
                        retained
                            .chunks
                            .try_reserve(1)
                            .map_err(|_| errors::memory_error(py))?;
                        retained.chunks.push((*start, previous.clone()));
                    }
                    *start = *end;
                    *end = next_end;
                    *current = Some(array);
                    if let Some(FastPath::Dictionary(reader)) = fast_path.as_deref_mut() {
                        reader.clear_chunk();
                    } else {
                        *fast_path = next_fast_path;
                    }
                }
                let array = current
                    .as_ref()
                    .ok_or_else(|| invalid(py, "Arrow stream has no current chunk"))?;
                Ok((array, row - *start))
            }
        }
    }
}

impl<'py> PreparedSource<'py> for Prepared<'py> {
    fn owner(&self) -> &Bound<'py, PyAny> {
        &self.owner
    }
    fn shape(&self) -> Shape {
        self.shape
    }
    fn prepare_field_names(&mut self, budget: &dyn Checkpoint) -> PyResult<bool> {
        let Shape::Table { columns, .. } = self.shape else {
            return Ok(false);
        };
        self.names.prepare_fields(0, columns, budget)?;
        Ok(true)
    }
    fn field_name(&self, column: usize) -> PyResult<Option<Bound<'py, PyAny>>> {
        self.names.field(0, column)
    }

    fn schema(&self, target: &mut dyn Target) -> PyResult<()> {
        consume_schema(self.owner.py(), self.field.data_type(), target)
    }

    fn child<'a>(
        &'a mut self,
        index: usize,
        budget: &dyn Checkpoint,
    ) -> PyResult<super::source::Child<'a, 'py>> {
        let shape = match self.shape {
            Shape::Table { rows, columns } if index < rows => Shape::Vector { length: columns },
            Shape::Vector { length } if index < length => {
                let (array, local) = self.storage.current_array(
                    self.owner.py(),
                    index,
                    &self.field,
                    self.shape,
                    &mut self.fast_path,
                    budget,
                )?;
                projection::shape(&self.owner, array, local, budget)?
            }
            _ => {
                return Err(invalid(
                    self.owner.py(),
                    "Arrow child is outside its prepared shape",
                ));
            }
        };
        Ok(super::source::Child::Native(super::source::Selected::new(
            self, index, shape,
        )))
    }

    fn consume_child(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        if matches!(self.shape, Shape::Table { .. }) {
            if column != 0 {
                return Err(invalid(
                    self.owner.py(),
                    "Arrow table row has only one positional axis",
                ));
            }
            return self.consume_cell(index, row, target, budget);
        }
        let (array, local) = self.storage.current_array(
            self.owner.py(),
            index,
            &self.field,
            self.shape,
            &mut self.fast_path,
            budget,
        )?;
        if row == 0
            && column == 0
            && (self.fast_path.is_some()
                || projection::shape(&self.owner, array, local, budget)? == Shape::Scalar)
        {
            return self.consume_cell(index, 0, target, budget);
        }
        projection::consume_child(
            Read {
                owner: &self.owner,
                offsets: &mut self.offsets,
                names: &mut self.names,
                node: 0,
            },
            array,
            local,
            row,
            column,
            target,
            budget,
        )
    }

    fn consume_cell(
        &mut self,
        row: usize,
        column: usize,
        target: &mut dyn Target,
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        self.consume_cell_to(row, column, target, budget)
    }

    crate::input::source::cell_destinations!();
}

fn consume_schema(py: Python<'_>, dtype: TypeView<'_>, target: &mut dyn Target) -> PyResult<()> {
    if binary_schema(dtype, &mut || target.checkpoint(py, 1))? {
        target.schema(py, Schema::Binary)?;
    }
    Ok(())
}
fn binary_schema(dtype: TypeView<'_>, check: &mut impl FnMut() -> PyResult<()>) -> PyResult<bool> {
    check()?;
    match dtype {
        TypeView::Scalar(
            DataType::Binary
            | DataType::LargeBinary
            | DataType::BinaryView
            | DataType::FixedSizeBinary(_),
        ) => Ok(true),
        TypeView::List(child)
        | TypeView::LargeList(child)
        | TypeView::FixedSizeList(child, _)
        | TypeView::Dictionary(_, child) => binary_schema(child.data_type(), check),
        TypeView::Struct(fields) => {
            for child in fields {
                if binary_schema(child.data_type(), check)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// Inspect an existing declared Arrow dtype, never an unused label array.
/// Reuse the same qualified public C Schema importer and admitted logical
/// kinds as actual arrays; targets still decide whether Binary is writable.
pub(crate) fn declared_schema(
    dtype: &Bound<'_, PyAny>,
    budget: &dyn Checkpoint,
) -> PyResult<Option<Schema>> {
    let py = dtype.py();
    let module = loaded_module(py)?
        .ok_or_else(|| invalid(py, "declared Arrow dtype has no loaded provider"))?;
    let base = crate::input::control::callback(py, Some(budget), || {
        errors::attribute(&module, c"DataType")
    })?
    .cast_into::<PyType>()?;
    if !super::actual_instance(dtype, &base) {
        return Err(invalid(
            py,
            "declared Arrow schema requires an actual DataType",
        ));
    }
    let actual = dtype.get_type();
    let name = actual.name()?;
    let library =
        crate::input::control::callback(py, Some(budget), || errors::attribute(&module, c"lib"))?;
    let canonical =
        crate::input::control::callback(py, Some(budget), || library.getattr_opt(&name))?;
    if !canonical.is_some_and(|canonical| actual.is(&canonical)) {
        return Err(invalid(
            py,
            "unsupported declared Arrow dtype subclass or extension schema",
        ));
    }
    let capsule = crate::input::control::callback(py, Some(budget), || {
        dtype.call_method0(c"__arrow_c_schema__")
    })?;
    // SAFETY: CPython checks the standard capsule name; the canonical provider
    // owns a live public C Schema. Moving it empties its release slot once.
    let schema = unsafe {
        let raw = pyo3::ffi::PyCapsule_GetPointer(capsule.as_ptr(), c"arrow_schema".as_ptr())
            .cast::<FFI_ArrowSchema>();
        if raw.is_null() {
            return Err(PyErr::fetch(py));
        }
        if (*raw).release().is_none() {
            return Err(invalid(py, "declared Arrow schema was already consumed"));
        }
        FFI_ArrowSchema::from_raw(raw)
    };
    let mut logical = names::Source::ArrowType(dtype.clone());
    let field = schema::import(py, &schema, &mut logical, budget)?;
    validate_field(py, &field, 0, budget)?;
    let binary = binary_schema(field.data_type(), &mut || super::check(py, budget))?;
    Ok(binary.then_some(Schema::Binary))
}

fn consume_scalar<'py>(
    py: Python<'py>,
    array: &ArrayRef,
    index: usize,
    target: &mut (impl Target + ?Sized),
    budget: &dyn Checkpoint,
    offsets: &mut Option<Box<offset::Offsets<'py>>>,
) -> PyResult<()> {
    target.checkpoint(py, 1)?;
    if index >= array.len() {
        return Err(invalid(py, "Arrow child index is outside its array"));
    }
    if array.is_null(index) {
        return target.scalar(py, ValueRef::Null);
    }
    macro_rules! native {
        ($kind:ty) => {
            array
                .as_any()
                .downcast_ref::<$kind>()
                .ok_or_else(|| invalid(py, "Arrow logical and physical array types differ"))?
                .value(index)
        };
    }
    match array
        .data_type()
        .scalar()
        .ok_or_else(|| invalid(py, "Arrow compound reached the scalar input boundary"))?
    {
        DataType::Null => target.scalar(py, ValueRef::Null),
        DataType::Boolean => target.scalar(py, ValueRef::Boolean(native!(BooleanArray))),
        DataType::Int8 => target.scalar(py, ValueRef::Signed(i64::from(native!(Int8Array)))),
        DataType::Int16 => target.scalar(py, ValueRef::Signed(i64::from(native!(Int16Array)))),
        DataType::Int32 => target.scalar(py, ValueRef::Signed(i64::from(native!(Int32Array)))),
        DataType::Int64 => target.scalar(py, ValueRef::Signed(native!(Int64Array))),
        DataType::UInt8 => target.scalar(py, ValueRef::Unsigned(u64::from(native!(UInt8Array)))),
        DataType::UInt16 => target.scalar(py, ValueRef::Unsigned(u64::from(native!(UInt16Array)))),
        DataType::UInt32 => target.scalar(py, ValueRef::Unsigned(u64::from(native!(UInt32Array)))),
        DataType::UInt64 => target.scalar(py, ValueRef::Unsigned(native!(UInt64Array))),
        DataType::Float32 => target.scalar(py, ValueRef::Float(f64::from(native!(Float32Array)))),
        DataType::Float64 => target.scalar(py, ValueRef::Float(native!(Float64Array))),
        DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Utf8View
        | DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_) => consume_bytes(py, array, index, target, budget),
        DataType::Date32 => target.scalar(py, ValueRef::DateDays(i64::from(native!(Date32Array)))),
        DataType::Date64 => {
            let value = native!(Date64Array);
            if value % 86_400_000 != 0 {
                return Err(errors::client(
                    py,
                    ClientErrorCode::NumericRange,
                    "Arrow date64 is not an integral day",
                ));
            }
            target.scalar(py, ValueRef::DateDays(value / 86_400_000))
        }
        DataType::Timestamp(unit, zone) => {
            let value = match unit {
                ArrowTimeUnit::Second => native!(TimestampSecondArray),
                ArrowTimeUnit::Millisecond => native!(TimestampMillisecondArray),
                ArrowTimeUnit::Microsecond => native!(TimestampMicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(TimestampNanosecondArray),
            };
            let fact = if let Some(zone) = zone {
                ValueRef::UtcTicks {
                    value,
                    unit: time_unit(unit),
                    multiplier: 1,
                    offset_micros: if target.needs_local_offsets() {
                        if offsets.is_none() {
                            *offsets = Some(super::boxed(py, offset::Offsets::default())?);
                        }
                        Some(
                            offsets
                                .as_mut()
                                .ok_or_else(|| {
                                    invalid(py, "Arrow timezone source was not prepared")
                                })?
                                .read(py, zone, value, unit, budget)?,
                        )
                    } else {
                        None
                    },
                }
            } else {
                ValueRef::Ticks {
                    value,
                    unit: time_unit(unit),
                    multiplier: 1,
                    kind: TimeKind::DateTime,
                }
            };
            target.scalar(py, fact)
        }
        DataType::Duration(unit) => {
            let value = match unit {
                ArrowTimeUnit::Second => native!(DurationSecondArray),
                ArrowTimeUnit::Millisecond => native!(DurationMillisecondArray),
                ArrowTimeUnit::Microsecond => native!(DurationMicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(DurationNanosecondArray),
            };
            target.scalar(
                py,
                ValueRef::Ticks {
                    value,
                    unit: time_unit(unit),
                    multiplier: 1,
                    kind: TimeKind::Duration,
                },
            )
        }
        DataType::Time32(unit) => {
            let value = match unit {
                ArrowTimeUnit::Second => i64::from(native!(Time32SecondArray)),
                ArrowTimeUnit::Millisecond => i64::from(native!(Time32MillisecondArray)),
                _ => return Err(invalid(py, "unsupported Arrow time32 unit")),
            };
            target.scalar(
                py,
                ValueRef::ClockTicks {
                    value,
                    unit: time_unit(unit),
                    multiplier: 1,
                },
            )
        }
        DataType::Time64(unit) => {
            let value = match unit {
                ArrowTimeUnit::Microsecond => native!(Time64MicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(Time64NanosecondArray),
                _ => return Err(invalid(py, "unsupported Arrow time64 unit")),
            };
            target.scalar(
                py,
                ValueRef::ClockTicks {
                    value,
                    unit: time_unit(unit),
                    multiplier: 1,
                },
            )
        }
        DataType::Decimal32(_, scale) => {
            consume_decimal(py, &native!(Decimal32Array).to_le_bytes(), *scale, target)
        }
        DataType::Decimal64(_, scale) => {
            consume_decimal(py, &native!(Decimal64Array).to_le_bytes(), *scale, target)
        }
        DataType::Decimal128(_, scale) => {
            consume_decimal(py, &native!(Decimal128Array).to_le_bytes(), *scale, target)
        }
        DataType::Decimal256(_, scale) => {
            consume_decimal(py, &native!(Decimal256Array).to_le_bytes(), *scale, target)
        }
        _ => Err(invalid(
            py,
            "unsupported Arrow scalar at the shared input boundary",
        )),
    }
}

fn consume_decimal(
    py: Python<'_>,
    bytes: &[u8],
    scale: i8,
    target: &mut (impl Target + ?Sized),
) -> PyResult<()> {
    let value = sqlaero_transcode::decimal::Coefficient::from_signed_le(bytes)
        .map_err(|message| errors::client(py, ClientErrorCode::NumericRange, message))?;
    target.scalar(py, value.scalar(scale))
}

fn cell_array<'a>(
    py: Python<'_>,
    array: &'a ArrayRef,
    table: bool,
    column: usize,
) -> PyResult<&'a ArrayRef> {
    if table {
        let data = array
            .as_any()
            .downcast_ref::<StructArray>()
            .ok_or_else(|| invalid(py, "Arrow table did not produce a Struct array"))?;
        Ok(data.column(column))
    } else {
        Ok(array)
    }
}

/// Consume one already resolved native text label through the common reader.
/// The target owns formatting and any successful encoded-byte reuse.
pub(crate) fn consume_text_reference(
    py: Python<'_>,
    reference: super::cell::TextReference<'_>,
    target: &mut dyn Target,
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    consume_scalar(
        py,
        reference.values,
        reference.index,
        target,
        budget,
        &mut None,
    )
}

/// Borrow only during native reading. No returned slice is retained across a
/// Python checkpoint; a live conforming producer must keep its exported buffers
/// allocated for the entire C array lifetime.
fn byte_slice<'a>(py: Python<'_>, array: &'a ArrayRef, index: usize) -> PyResult<&'a [u8]> {
    if let Some(bytes) = array.as_any().downcast_ref::<native::Bytes>() {
        return bytes
            .value(index)
            .ok_or_else(|| invalid(py, "Arrow byte value offsets exceed its data buffer"));
    }
    let view_array = array
        .as_any()
        .downcast_ref::<native::View>()
        .ok_or_else(|| invalid(py, "Arrow byte storage physical type differs"))?;
    let view = view_array
        .views()
        .get(index)
        .ok_or_else(|| invalid(py, "Arrow byte view index exceeds its view buffer"))?;
    validate_binary_view(std::slice::from_ref(view), view_array.data_buffers())
        .map_err(|error| errors::builtin_format::<PyTypeError>(py, format_args!("{error}")))?;
    let length = (*view as u32) as usize;
    if length <= 12 {
        // SAFETY: the mature single-view validator accepted this inline view;
        // the selected length is within its fixed 12-byte inline storage.
        return Ok(unsafe { arrow_array::BinaryViewArray::inline_value(view, length) });
    }
    let view = ByteView::from(*view);
    let start = view.offset as usize;
    let end = start
        .checked_add(length)
        .ok_or_else(|| invalid(py, "Arrow byte view length overflows usize"))?;
    view_array
        .data_buffers()
        .get(view.buffer_index as usize)
        .and_then(|buffer| buffer.get(start..end))
        .ok_or_else(|| invalid(py, "Arrow byte view exceeds its data buffer"))
}

fn utf8<'a>(py: Python<'_>, bytes: &'a [u8], start: usize) -> PyResult<&'a str> {
    std::str::from_utf8(bytes).map_err(|error| utf8_error(py, error, start))
}

fn utf8_error(py: Python<'_>, error: std::str::Utf8Error, start: usize) -> PyErr {
    errors::builtin_format::<PyTypeError>(
        py,
        format_args!(
            "Arrow UTF-8 value is invalid at byte {}",
            start.saturating_add(error.valid_up_to())
        ),
    )
}

fn consume_bytes(
    py: Python<'_>,
    array: &ArrayRef,
    index: usize,
    target: &mut (impl Target + ?Sized),
    budget: &dyn Checkpoint,
) -> PyResult<()> {
    let text = matches!(
        array.data_type().scalar(),
        Some(DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
    );
    let (length, short) = {
        let source = byte_slice(py, array, index)?;
        let length = source.len();
        let short = if length <= super::WORK_BYTES {
            Some(if text {
                std::str::from_utf8(source).map(|value| target.native_scalar(ValueRef::Text(value)))
            } else {
                Ok(target.native_scalar(ValueRef::Bytes(source)))
            })
        } else {
            None
        };
        (length, short)
    };
    let failure = |error: NativeFailure, start| match error {
        NativeFailure::Mysql(ValueError::Encoding(error)) => byte_codec_error(
            py,
            array,
            index,
            start,
            error,
            target.text_encoding(),
            budget,
        ),
        error => error.python(py),
    };
    if let Some(result) = short {
        return result
            .map_err(|error| utf8_error(py, error, 0))?
            .map_err(|error| failure(error, 0));
    }
    if !text {
        target.binary_minimum(py, length)?;
    }
    // Allocate a bounded owned fragment before any callback. Only that scratch
    // reaches Python diagnostics; successful small values were never copied.
    let mut scratch = Vec::new();
    scratch
        .try_reserve(super::WORK_BYTES + 3)
        .map_err(|_| errors::memory_error(py))?;
    super::check(py, budget)?;
    if text {
        target
            .native_text_start(length)
            .map_err(|error| error.python(py))?;
    } else {
        target.bytes_start(py, length)?;
    }
    let mut start = 0;
    while start < length {
        super::check(py, budget)?;
        let end = {
            let source = byte_slice(py, array, index)?;
            if source.len() != length {
                return Err(invalid(
                    py,
                    "Arrow byte value changed length during encoding",
                ));
            }
            let mut end = (start + super::WORK_BYTES).min(length);
            if text {
                for _ in 0..3 {
                    if end < length && source[end] & 0xc0 == 0x80 {
                        end += 1;
                    } else {
                        break;
                    }
                }
            }
            scratch.clear();
            scratch.extend_from_slice(&source[start..end]);
            end
        };
        if text {
            if let Err(error) = target.native_text_fragment(utf8(py, &scratch, start)?) {
                return Err(match error {
                    NativeFailure::Mysql(ValueError::Encoding(error)) => byte_codec_error(
                        py,
                        array,
                        index,
                        start,
                        error,
                        target.text_encoding(),
                        budget,
                    ),
                    error => error.python(py),
                });
            }
        } else {
            target.bytes_fragment(py, &scratch)?;
        }
        start = end;
    }
    if text {
        target.native_text_end().map_err(|error| error.python(py))
    } else {
        target.bytes_end(py)
    }
}

/// Reconstruct the source only after an actual connection-encoding failure.
/// The successful path neither materializes a whole value nor creates str.
fn byte_codec_error(
    py: Python<'_>,
    array: &ArrayRef,
    index: usize,
    start: usize,
    error: CodecError,
    encoding: &str,
    budget: &dyn Checkpoint,
) -> PyErr {
    let error = match error {
        CodecError::Encode { offset, point } => CodecError::Encode {
            offset: start + offset,
            point,
        },
        CodecError::Wire(error) => return errors::wire(py, error),
        error => {
            return errors::client_format(
                py,
                ClientErrorCode::UnsupportedCharset,
                format_args!("{error}"),
            );
        }
    };
    let original = (|| {
        let length = byte_slice(py, array, index)?.len();
        let mut owned = Vec::new();
        owned
            .try_reserve(length)
            .map_err(|_| errors::memory_error(py))?;
        let mut position = 0;
        while position < length {
            super::check(py, budget)?;
            let end = {
                let source = byte_slice(py, array, index)?;
                if source.len() != length {
                    None
                } else {
                    let mut end = (position + super::WORK_BYTES).min(length);
                    for _ in 0..3 {
                        if end < length && source[end] & 0xc0 == 0x80 {
                            end += 1;
                        } else {
                            break;
                        }
                    }
                    owned.extend_from_slice(&source[position..end]);
                    Some(end)
                }
            }
            .ok_or_else(|| invalid(py, "Arrow value changed during encoding-error diagnosis"))?;
            // Validate each complete owned fragment after the external borrow
            // ends. Error creation/checkpoints never borrow producer bytes.
            utf8(py, &owned[position..end], position)?;
            position = end;
        }
        // SAFETY: the disjoint complete-character fragments above cover all
        // owned bytes and each passed UTF-8 validation. No second unbounded
        // validation or Python UTF-8-cache conversion is needed.
        let text = unsafe { std::str::from_utf8_unchecked(&owned) };
        let original = super::unicode::from_utf8(py, text, budget)?;
        Ok(super::codec_error(
            &original,
            text,
            encoding,
            error,
            Some(budget),
        ))
    })();
    match original {
        Ok(error) | Err(error) => error,
    }
}

/// Resolve one actual key against its own dictionary, for both the ordinary
/// recursive writer and the bounded native-string label cache.
fn dictionary_label<'a>(
    py: Python<'_>,
    array: &'a ArrayRef,
    index: usize,
) -> PyResult<(&'a ArrayRef, usize)> {
    macro_rules! label {
        ($key:ty) => {{
            let dictionary = array
                .as_any()
                .downcast_ref::<native::Dictionary>()
                .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
            let keys = dictionary
                .keys()
                .as_any()
                .downcast_ref::<arrow_array::PrimitiveArray<$key>>()
                .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
            let key = keys.value(index);
            let label = usize::try_from(key)
                .map_err(|_| invalid(py, "Arrow Dictionary key is negative or exceeds usize"))?;
            if label >= dictionary.values().len() {
                return Err(invalid(
                    py,
                    "Arrow Dictionary key is outside the current dictionary",
                ));
            }
            Ok((dictionary.values(), label))
        }};
    }
    match array.data_type() {
        TypeView::Dictionary(key, _) => match key {
            DataType::Int8 => label!(Int8Type),
            DataType::Int16 => label!(Int16Type),
            DataType::Int32 => label!(Int32Type),
            DataType::Int64 => label!(Int64Type),
            DataType::UInt8 => label!(UInt8Type),
            DataType::UInt16 => label!(UInt16Type),
            DataType::UInt32 => label!(UInt32Type),
            DataType::UInt64 => label!(UInt64Type),
            _ => Err(invalid(py, "unsupported Arrow Dictionary key type")),
        },
        _ => Err(invalid(py, "Arrow value is not a Dictionary")),
    }
}

fn time_unit(unit: &ArrowTimeUnit) -> TimeUnit {
    match unit {
        ArrowTimeUnit::Second => TimeUnit::Second,
        ArrowTimeUnit::Millisecond => TimeUnit::Millisecond,
        ArrowTimeUnit::Microsecond => TimeUnit::Microsecond,
        ArrowTimeUnit::Nanosecond => TimeUnit::Nanosecond,
    }
}

impl<'py> Prepared<'py> {
    fn consume_cell_to(
        &mut self,
        row: usize,
        column: usize,
        target: &mut (impl Target + ?Sized),
        budget: &dyn Checkpoint,
    ) -> PyResult<()> {
        let py = self.owner.py();
        target.checkpoint(py, 1)?;
        let (array, index) = self.storage.current_array(
            py,
            row,
            &self.field,
            self.shape,
            &mut self.fast_path,
            budget,
        )?;
        let table = match self.shape {
            Shape::Table { columns, .. } if column < columns => true,
            Shape::Vector { .. } if column == 0 => false,
            _ => return Err(invalid(py, "Arrow cell is outside its prepared shape")),
        };
        if let Some(reader) = self.fast_path.as_deref_mut() {
            let value = match reader {
                FastPath::Dictionary(reader) if reader.has_column(column) => {
                    let value = cell_array(py, array, table, column)?;
                    let Some(reference) = reader.reference(py, value, index, column)? else {
                        return target.scalar(py, ValueRef::Null);
                    };
                    if target.referenced_text(py, reference, budget)? {
                        return Ok(());
                    }
                    return consume_scalar(
                        py,
                        reference.values,
                        reference.index,
                        target,
                        budget,
                        &mut self.offsets,
                    );
                }
                FastPath::Dictionary(_) => None,
                FastPath::Int64(reader) => Some(reader.read(py, index, column)?),
                FastPath::Float32(reader) => Some(reader.read(py, index)?),
                FastPath::Float64(reader) => Some(reader.read(py, index)?),
                FastPath::Boolean(reader) => Some(reader.read(py, index)?),
            };
            if let Some(value) = value {
                // Native buffer borrows end before target error/control work.
                return target.scalar(py, value);
            }
        }
        let value = cell_array(py, array, table, column)?;
        let node = if table && target.needs_field_names() && names::needed(value.data_type()) {
            self.names.child(0, names::Edge::Field(column), budget)?
        } else {
            0
        };
        consume_value(
            Read {
                owner: &self.owner,
                offsets: &mut self.offsets,
                names: &mut self.names,
                node,
            },
            value,
            index,
            target,
            budget,
        )
    }

    fn write_cell_to(
        &mut self,
        row: usize,
        column: usize,
        output: &mut (impl Destination + ?Sized),
        context: &Context,
        budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        self.consume_cell_to(
            row,
            column,
            &mut super::cell::Mysql {
                output,
                context,
                visitor,
            },
            budget,
        )
    }
}
