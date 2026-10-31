//! Canonical CPU Arrow sources, imported through the public C Data interface.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

use arrow_array::ffi::{FFI_ArrowArray, from_ffi_and_data_type};
use arrow_array::types::{
    Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use arrow_array::{
    Array, ArrayRef, BinaryArray, BinaryViewArray, BooleanArray, Date32Array, Date64Array,
    Decimal32Array, Decimal64Array, Decimal128Array, Decimal256Array, DictionaryArray,
    DurationMicrosecondArray, DurationMillisecondArray, DurationNanosecondArray,
    DurationSecondArray, FixedSizeBinaryArray, FixedSizeListArray, Float32Array, Float64Array,
    Int8Array, Int16Array, Int32Array, Int64Array, LargeBinaryArray, LargeListArray,
    LargeStringArray, ListArray, StringArray, StringViewArray, StructArray, Time32MillisecondArray,
    Time32SecondArray, Time64MicrosecondArray, Time64NanosecondArray, TimestampMicrosecondArray,
    TimestampMillisecondArray, TimestampNanosecondArray, TimestampSecondArray, UInt8Array,
    UInt16Array, UInt32Array, UInt64Array, make_array,
};
use arrow_data::{ByteView, validate_binary_view};
use arrow_schema::{DataType, Field, TimeUnit as ArrowTimeUnit, ffi::FFI_ArrowSchema};
use pyo3::exceptions::{PyBufferError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple, PyType};
use sqlaero_core::bind::{Context, scalar, writer};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::deadline::IoBudget;

use super::source::{CellWriter, PreparedSource, Shape};
use super::temporal::{self, TimeKind, TimeUnit};
use crate::errors;

mod cdata;
mod dictionary;

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

/// The published C Schema ABI, used only to reject released, deeply recursive
/// or text-invalid schemas before the mature Rust importer traverses them.
#[repr(C)]
struct SchemaAbi {
    format: *const c_char,
    name: *const c_char,
    metadata: *const c_char,
    flags: i64,
    n_children: i64,
    children: *const *const Self,
    dictionary: *const Self,
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

enum Storage {
    Single(ArrayRef),
    Stream {
        stream: StreamAbi,
        current: Option<ArrayRef>,
        start: usize,
        end: usize,
    },
}

/// Boundary-owned source. Rust Arrow buffers own the moved C release callback;
/// the Python owner and token keep provider access/release in the right domain.
pub(crate) struct Prepared<'py> {
    storage: Storage,
    dictionary: Option<Box<dictionary::Cache>>,
    field: Field,
    shape: Shape,
    owner: Bound<'py, PyAny>,
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

fn preflight_schema(py: Python<'_>, schema: &FFI_ArrowSchema, budget: &IoBudget) -> PyResult<()> {
    // SAFETY: Apache's repr(C) schema implements the public C Schema ABI.
    // The live canonical producer owns every referenced child and C string.
    let raw = unsafe { &*(ptr::from_ref(schema).cast::<SchemaAbi>()) };
    preflight_schema_node(py, raw, 0, &mut 0, budget)
}

fn preflight_schema_node(
    py: Python<'_>,
    schema: &SchemaAbi,
    depth: usize,
    work: &mut usize,
    budget: &IoBudget,
) -> PyResult<()> {
    if depth >= 64 {
        return Err(errors::client(
            py,
            ClientErrorCode::ContainerNesting,
            "Arrow schema exceeds 64 nested container levels",
        ));
    }
    *work += 1;
    if *work >= 1024 {
        super::check(py, budget)?;
        *work = 0;
    }
    if schema.release.is_none() || schema.format.is_null() {
        return Err(invalid(py, "Arrow schema is released or has no format"));
    }
    // SAFETY: the standard live schema provides NUL-terminated format/name
    // strings. Validate UTF-8 before Arrow's public, asserting text accessors.
    unsafe { CStr::from_ptr(schema.format) }
        .to_str()
        .map_err(|_| invalid(py, "Arrow schema format is not valid UTF-8"))?;
    if !schema.name.is_null() {
        // SAFETY: same live C Schema contract as the format above.
        unsafe { CStr::from_ptr(schema.name) }
            .to_str()
            .map_err(|_| invalid(py, "Arrow schema name is not valid UTF-8"))?;
    }
    let children = usize::try_from(schema.n_children)
        .map_err(|_| invalid(py, "Arrow schema has a negative child count"))?;
    if children != 0 && schema.children.is_null() {
        return Err(invalid(
            py,
            "Arrow schema is missing its child pointer array",
        ));
    }
    for index in 0..children {
        // SAFETY: the C Schema contract provides n_children live pointers;
        // null entries are rejected before any pointed-to object is read.
        let child = unsafe { *schema.children.add(index) };
        // SAFETY: a non-null child belongs to the same live standard schema.
        let Some(child) = (unsafe { child.as_ref() }) else {
            return Err(invalid(py, "Arrow schema contains a null child pointer"));
        };
        preflight_schema_node(py, child, depth + 1, work, budget)?;
    }
    // SAFETY: a non-null dictionary is another live schema under the same
    // producer owner, rather than a pointer into an unrelated Python layout.
    if let Some(dictionary) = unsafe { schema.dictionary.as_ref() } {
        preflight_schema_node(py, dictionary, depth + 1, work, budget)?;
        // Dictionary values become a bare DataType in Arrow's logical model;
        // their field metadata would otherwise be discarded before our normal
        // recursive Field validation can see an extension declaration.
        // SAFETY: this node is the same live public C Schema ABI as above.
        let dictionary = unsafe { &*(ptr::from_ref(dictionary).cast::<FFI_ArrowSchema>()) };
        if dictionary
            .metadata()
            .map_err(|error| invalid(py, error.to_string()))?
            .contains_key("ARROW:extension:name")
        {
            return Err(invalid(
                py,
                "Arrow dictionary extension values are outside the supported input contract",
            ));
        }
    }
    Ok(())
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
    budget: &IoBudget,
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
    budget: &IoBudget,
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
    prepare_exported(value, value, shape, budget).map(Some)
}

/// Import a public CPU export whose canonical outer type was already checked.
/// Polars calls this after separating Object columns and preserving its shape.
pub(crate) fn prepare_exported<'py>(
    owner: &Bound<'py, PyAny>,
    exported: &Bound<'py, PyAny>,
    shape: Shape,
    budget: &IoBudget,
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
        let (schema, mut array) = unsafe {
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
        preflight_schema(py, &schema, budget)?;
        let field = Field::try_from(&schema).map_err(|error| invalid(py, error.to_string()))?;
        validate_field(py, &field, 0)?;
        super::check(py, budget)?;
        cdata::normalize_nulls(py, &mut array, field.data_type(), budget)?;
        // SAFETY: a canonical CPU producer owns these valid C buffers until
        // the moved array's release. The importer handles offsets/alignment.
        let data = unsafe { from_ffi_and_data_type(array, field.data_type().clone()) }
            .map_err(|error| invalid(py, error.to_string()))?;
        data.validate()
            .map_err(|error| invalid(py, error.to_string()))?;
        let array = make_array(data);
        validate_root_shape(py, &array, shape)?;
        return Ok(Prepared {
            storage: Storage::Single(array),
            dictionary: dictionary::Cache::for_schema(py, &field, shape)?,
            field,
            shape,
            owner: owner.clone(),
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
    preflight_schema(py, &schema, budget)?;
    let field = Field::try_from(&schema).map_err(|error| invalid(py, error.to_string()))?;
    validate_field(py, &field, 0)?;
    validate_table_schema(py, &field, shape)?;
    super::check(py, budget)?;
    Ok(Prepared {
        storage: Storage::Stream {
            stream,
            current: None,
            start: 0,
            end: 0,
        },
        dictionary: dictionary::Cache::for_schema(py, &field, shape)?,
        field,
        shape,
        owner: owner.clone(),
    })
}

fn validate_table_schema(py: Python<'_>, field: &Field, shape: Shape) -> PyResult<()> {
    if let Shape::Table { columns, .. } = shape {
        match field.data_type() {
            DataType::Struct(fields) if fields.len() == columns => {}
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
                && matches!(array.data_type(), DataType::Struct(fields) if fields.len() == columns) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            py,
            "Arrow array does not match its prepared logical shape",
        )),
    }
}

fn validate_field(py: Python<'_>, field: &Field, depth: usize) -> PyResult<()> {
    if depth >= 64 {
        return Err(errors::client(
            py,
            ClientErrorCode::ContainerNesting,
            "Arrow schema exceeds 64 nested container levels",
        ));
    }
    if field.metadata().contains_key("ARROW:extension:name") {
        return Err(invalid(
            py,
            "Arrow extension storage is outside the supported input contract",
        ));
    }
    match field.data_type() {
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
        DataType::List(child) | DataType::LargeList(child) => validate_field(py, child, depth + 1),
        DataType::FixedSizeList(child, width) if *width >= 0 => {
            validate_field(py, child, depth + 1)
        }
        DataType::Struct(fields) => {
            for child in fields {
                validate_field(py, child, depth + 1)?;
            }
            Ok(())
        }
        DataType::Dictionary(key, value) => {
            if !matches!(
                key.as_ref(),
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
            validate_field(py, &Field::new("", value.as_ref().clone(), true), depth + 1)
        }
        _ => Err(invalid(py, "unsupported Arrow logical input dtype")),
    }
}

impl Prepared<'_> {
    fn current_array(&mut self, row: usize, budget: &IoBudget) -> PyResult<(ArrayRef, usize)> {
        let py = self.owner.py();
        match &mut self.storage {
            Storage::Single(array) => Ok((array.clone(), row)),
            Storage::Stream {
                stream,
                current,
                start,
                end,
            } => {
                if row < *start {
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
                    cdata::normalize_nulls(py, &mut raw, self.field.data_type(), budget)?;
                    // SAFETY: canonical CPU stream's buffers follow the C
                    // interface and retain their own moved release callback.
                    let data =
                        unsafe { from_ffi_and_data_type(raw, self.field.data_type().clone()) }
                            .map_err(|error| invalid(py, error.to_string()))?;
                    data.validate()
                        .map_err(|error| invalid(py, error.to_string()))?;
                    let array = make_array(data);
                    *start = *end;
                    *end = end
                        .checked_add(array.len())
                        .ok_or_else(|| invalid(py, "Arrow stream length overflows usize"))?;
                    *current = Some(array);
                    if let Some(cache) = &mut self.dictionary {
                        cache.clear_chunk();
                    }
                }
                let array = current
                    .as_ref()
                    .ok_or_else(|| invalid(py, "Arrow stream has no current chunk"))?;
                Ok((array.clone(), row - *start))
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

    fn write_cell(
        &mut self,
        row: usize,
        column: usize,
        output: &mut Vec<u8>,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let py = self.owner.py();
        visitor.checkpoint(py, 1)?;
        let (array, index) = self.current_array(row, budget)?;
        if index >= array.len() {
            return Err(invalid(py, "Arrow source row is outside its current array"));
        }
        if let Shape::Table { columns, .. } = self.shape {
            if column >= columns {
                return Err(invalid(
                    py,
                    "Arrow source column is outside its prepared schema",
                ));
            }
            let data = array
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| invalid(py, "Arrow table did not produce a Struct array"))?;
            let value = data.column(column);
            if let Some(cache) = &mut self.dictionary
                && cache.has_column(column)
            {
                return cache.write(py, column, value, index, output, context, budget, visitor);
            }
            write_value(py, value, index, output, context, budget, visitor)
        } else {
            if column != 0 {
                return Err(invalid(
                    py,
                    "Arrow vector has only one logical cell per element",
                ));
            }
            if let Some(cache) = &mut self.dictionary {
                return cache.write(py, 0, &array, index, output, context, budget, visitor);
            }
            write_value(py, &array, index, output, context, budget, visitor)
        }
    }
}

fn write_value(
    py: Python<'_>,
    array: &ArrayRef,
    index: usize,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &IoBudget,
    visitor: &mut dyn CellWriter,
) -> PyResult<()> {
    visitor.checkpoint(py, 1)?;
    if index >= array.len() {
        return Err(invalid(py, "Arrow child index is outside its array"));
    }
    if array.is_null(index) {
        return writer::append(output, b"NULL", context.limit)
            .map_err(|error| errors::wire(py, error));
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
    match array.data_type() {
        DataType::Null => {
            writer::append(output, b"NULL", context.limit).map_err(|error| errors::wire(py, error))
        }
        DataType::Boolean => {
            scalar::integer(output, u8::from(native!(BooleanArray)), context.limit)
                .map_err(|error| errors::wire(py, error))
        }
        DataType::Int8 => scalar::integer(output, native!(Int8Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Int16 => scalar::integer(output, native!(Int16Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Int32 => scalar::integer(output, native!(Int32Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Int64 => scalar::integer(output, native!(Int64Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::UInt8 => scalar::integer(output, native!(UInt8Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::UInt16 => scalar::integer(output, native!(UInt16Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::UInt32 => scalar::integer(output, native!(UInt32Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::UInt64 => scalar::integer(output, native!(UInt64Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Float32 => scalar::float(output, f64::from(native!(Float32Array)), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Float64 => scalar::float(output, native!(Float64Array), context.limit)
            .map_err(|error| errors::wire(py, error)),
        DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Utf8View
        | DataType::Binary
        | DataType::LargeBinary
        | DataType::BinaryView
        | DataType::FixedSizeBinary(_) => write_bytes(py, array, index, output, context, budget),
        _ => write_extended(py, array, index, output, context, budget, visitor),
    }
}

/// Borrow only during native reading. No returned slice is retained across a
/// Python checkpoint; a live conforming producer must keep its exported buffers
/// allocated for the entire C array lifetime.
fn byte_slice<'a>(py: Python<'_>, array: &'a ArrayRef, index: usize) -> PyResult<&'a [u8]> {
    macro_rules! offsets {
        ($kind:ty) => {{
            let array = array
                .as_any()
                .downcast_ref::<$kind>()
                .ok_or_else(|| invalid(py, "Arrow logical and physical byte array types differ"))?;
            let offsets = array.value_offsets();
            let start = usize::try_from(offsets[index])
                .map_err(|_| invalid(py, "Arrow byte value has a negative start offset"))?;
            let end = usize::try_from(offsets[index + 1])
                .map_err(|_| invalid(py, "Arrow byte value has a negative end offset"))?;
            array
                .value_data()
                .get(start..end)
                .ok_or_else(|| invalid(py, "Arrow byte value offsets exceed its data buffer"))
        }};
    }
    macro_rules! view {
        ($kind:ty) => {{
            let array = array
                .as_any()
                .downcast_ref::<$kind>()
                .ok_or_else(|| invalid(py, "Arrow byte view physical type differs"))?;
            let view = array
                .views()
                .get(index)
                .ok_or_else(|| invalid(py, "Arrow byte view index exceeds its view buffer"))?;
            validate_binary_view(std::slice::from_ref(view), array.data_buffers())
                .map_err(|error| invalid(py, error.to_string()))?;
            let length = (*view as u32) as usize;
            if length <= 12 {
                // SAFETY: the mature binary-view validator above accepted the
                // inline view, and its byte length is at most the inline bound.
                Ok(unsafe { <$kind>::inline_value(view, length) })
            } else {
                let view = ByteView::from(*view);
                let start = view.offset as usize;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| invalid(py, "Arrow byte view length overflows usize"))?;
                array
                    .data_buffers()
                    .get(view.buffer_index as usize)
                    .and_then(|buffer| buffer.get(start..end))
                    .ok_or_else(|| invalid(py, "Arrow byte view exceeds its data buffer"))
            }
        }};
    }
    match array.data_type() {
        DataType::Utf8 => offsets!(StringArray),
        DataType::LargeUtf8 => offsets!(LargeStringArray),
        DataType::Binary => offsets!(BinaryArray),
        DataType::LargeBinary => offsets!(LargeBinaryArray),
        DataType::Utf8View => view!(StringViewArray),
        DataType::BinaryView => view!(BinaryViewArray),
        DataType::FixedSizeBinary(_) => array
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .map(|array| array.value(index))
            .ok_or_else(|| invalid(py, "Arrow fixed binary physical type differs")),
        _ => Err(invalid(py, "Arrow value is not supported byte storage")),
    }
}

fn utf8<'a>(py: Python<'_>, bytes: &'a [u8], start: usize) -> PyResult<&'a str> {
    std::str::from_utf8(bytes).map_err(|error| {
        errors::builtin_format::<PyTypeError>(
            py,
            format_args!(
                "Arrow UTF-8 value is invalid at byte {}",
                start.saturating_add(error.valid_up_to())
            ),
        )
    })
}

fn write_bytes(
    py: Python<'_>,
    array: &ArrayRef,
    index: usize,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &IoBudget,
) -> PyResult<()> {
    let text = matches!(
        array.data_type(),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
    );
    let length = byte_slice(py, array, index)?.len();
    if length <= super::WORK_BYTES {
        let result = {
            let source = byte_slice(py, array, index)?;
            if text {
                context.codec.literal(
                    utf8(py, source, 0)?,
                    output,
                    context.no_backslash_escapes,
                    context.limit,
                )
            } else {
                return writer::binary(output, source, context.limit)
                    .map_err(|error| errors::wire(py, error));
            }
        };
        return result.map_err(|error| {
            byte_codec_error(py, array, index, 0, error, context.codec.name(), budget)
        });
    }
    if !text {
        writer::binary_width(output.len(), length, context.limit)
            .map_err(|error| errors::wire(py, error))?;
    }
    let mut scratch = Vec::new();
    scratch
        .try_reserve(super::WORK_BYTES + 3)
        .map_err(|_| errors::memory_error(py))?;
    super::check(py, budget)?;
    if text {
        writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))?;
    } else {
        writer::binary_start(output, length, context.limit)
            .map_err(|error| errors::wire(py, error))?;
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
        // The source borrow ended above. Only Rust-owned scratch bytes reach
        // encoding, error construction or the next Python signal checkpoint.
        if text {
            context
                .codec
                .literal_fragment(
                    utf8(py, &scratch, start)?,
                    output,
                    context.no_backslash_escapes,
                    context.limit,
                )
                .map_err(|error| {
                    byte_codec_error(py, array, index, start, error, context.codec.name(), budget)
                })?;
        } else {
            writer::binary_fragment(output, &scratch, context.limit)
                .map_err(|error| errors::wire(py, error))?;
        }
        start = end;
    }
    if text {
        writer::append(output, b"'", context.limit).map_err(|error| errors::wire(py, error))
    } else {
        writer::binary_end(output, context.limit).map_err(|error| errors::wire(py, error))
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
    budget: &IoBudget,
) -> PyErr {
    let error = match error {
        CodecError::Encode { offset, point } => CodecError::Encode {
            offset: start + offset,
            point,
        },
        CodecError::Wire(error) => return errors::wire(py, error),
        error => {
            return errors::client(py, ClientErrorCode::UnsupportedCharset, error.to_string());
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
            let end = (position + super::WORK_BYTES).min(length);
            {
                let source = byte_slice(py, array, index)?;
                let chunk = source.get(position..end).ok_or_else(|| {
                    invalid(py, "Arrow value changed during encoding-error diagnosis")
                })?;
                owned.extend_from_slice(chunk);
            }
            position = end;
        }
        let original = PyString::from_bytes(py, &owned)?;
        let text = original.to_str()?;
        Ok(super::codec_error(&original, text, encoding, error))
    })();
    match original {
        Ok(error) | Err(error) => error,
    }
}

fn write_extended(
    py: Python<'_>,
    array: &ArrayRef,
    index: usize,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &IoBudget,
    visitor: &mut dyn CellWriter,
) -> PyResult<()> {
    macro_rules! native {
        ($kind:ty) => {
            array
                .as_any()
                .downcast_ref::<$kind>()
                .ok_or_else(|| invalid(py, "Arrow logical and physical array types differ"))?
                .value(index)
        };
    }
    match array.data_type() {
        DataType::Date32 => temporal::write_date(
            py,
            i64::from(native!(Date32Array)),
            output,
            context,
            Some(budget),
        ),
        DataType::Date64 => {
            let millis = native!(Date64Array);
            if millis % 86_400_000 != 0 {
                return Err(errors::client(
                    py,
                    ClientErrorCode::NumericRange,
                    "Arrow date64 is not an integral day",
                ));
            }
            temporal::write_date(py, millis / 86_400_000, output, context, Some(budget))
        }
        DataType::Timestamp(unit, timezone) => {
            if timezone.is_some() && !context.utc_confirmed {
                return Err(errors::client(
                    py,
                    ClientErrorCode::SessionState,
                    "timezone-aware Arrow input requires a confirmed UTC MySQL session",
                ));
            }
            let ticks = match unit {
                ArrowTimeUnit::Second => native!(TimestampSecondArray),
                ArrowTimeUnit::Millisecond => native!(TimestampMillisecondArray),
                ArrowTimeUnit::Microsecond => native!(TimestampMicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(TimestampNanosecondArray),
            };
            temporal::write_valid(
                py,
                ticks,
                time_unit(unit),
                1,
                TimeKind::DateTime,
                output,
                context,
                Some(budget),
            )
        }
        DataType::Duration(unit) => {
            let ticks = match unit {
                ArrowTimeUnit::Second => native!(DurationSecondArray),
                ArrowTimeUnit::Millisecond => native!(DurationMillisecondArray),
                ArrowTimeUnit::Microsecond => native!(DurationMicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(DurationNanosecondArray),
            };
            temporal::write_valid(
                py,
                ticks,
                time_unit(unit),
                1,
                TimeKind::Duration,
                output,
                context,
                Some(budget),
            )
        }
        DataType::Time32(unit) => {
            let ticks = match unit {
                ArrowTimeUnit::Second => i64::from(native!(Time32SecondArray)),
                ArrowTimeUnit::Millisecond => i64::from(native!(Time32MillisecondArray)),
                _ => return Err(invalid(py, "unsupported Arrow time32 unit")),
            };
            temporal::write_clock(py, ticks, time_unit(unit), 1, output, context, Some(budget))
        }
        DataType::Time64(unit) => {
            let ticks = match unit {
                ArrowTimeUnit::Microsecond => native!(Time64MicrosecondArray),
                ArrowTimeUnit::Nanosecond => native!(Time64NanosecondArray),
                _ => return Err(invalid(py, "unsupported Arrow time64 unit")),
            };
            temporal::write_clock(py, ticks, time_unit(unit), 1, output, context, Some(budget))
        }
        DataType::Decimal32(_, scale) => super::decimal_bytes(
            py,
            &native!(Decimal32Array).to_le_bytes(),
            *scale,
            output,
            context,
        ),
        DataType::Decimal64(_, scale) => super::decimal_bytes(
            py,
            &native!(Decimal64Array).to_le_bytes(),
            *scale,
            output,
            context,
        ),
        DataType::Decimal128(_, scale) => super::decimal_bytes(
            py,
            &native!(Decimal128Array).to_le_bytes(),
            *scale,
            output,
            context,
        ),
        DataType::Decimal256(_, scale) => super::decimal_bytes(
            py,
            &native!(Decimal256Array).to_le_bytes(),
            *scale,
            output,
            context,
        ),
        DataType::List(_) => {
            let list = array
                .as_any()
                .downcast_ref::<ListArray>()
                .ok_or_else(|| invalid(py, "Arrow List physical type differs"))?;
            let offsets = list.value_offsets();
            let start = usize::try_from(offsets[index])
                .map_err(|_| invalid(py, "Arrow List has a negative start offset"))?;
            let end = usize::try_from(offsets[index + 1])
                .map_err(|_| invalid(py, "Arrow List has a negative end offset"))?;
            write_list(
                py,
                list.values(),
                start,
                end,
                output,
                context,
                budget,
                visitor,
            )
        }
        DataType::LargeList(_) => {
            let list = array
                .as_any()
                .downcast_ref::<LargeListArray>()
                .ok_or_else(|| invalid(py, "Arrow LargeList physical type differs"))?;
            let offsets = list.value_offsets();
            let start = usize::try_from(offsets[index])
                .map_err(|_| invalid(py, "Arrow LargeList has a negative start offset"))?;
            let end = usize::try_from(offsets[index + 1])
                .map_err(|_| invalid(py, "Arrow LargeList has a negative end offset"))?;
            write_list(
                py,
                list.values(),
                start,
                end,
                output,
                context,
                budget,
                visitor,
            )
        }
        DataType::FixedSizeList(_, _) => {
            let list = array
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .ok_or_else(|| invalid(py, "Arrow FixedSizeList physical type differs"))?;
            let width = usize::try_from(list.value_length())
                .map_err(|_| invalid(py, "Arrow FixedSizeList width is negative"))?;
            let start = index
                .checked_mul(width)
                .ok_or_else(|| invalid(py, "Arrow FixedSizeList offset overflows usize"))?;
            let end = start
                .checked_add(width)
                .ok_or_else(|| invalid(py, "Arrow FixedSizeList end overflows usize"))?;
            write_list(
                py,
                list.values(),
                start,
                end,
                output,
                context,
                budget,
                visitor,
            )
        }
        DataType::Struct(_) => {
            let fields = array
                .as_any()
                .downcast_ref::<StructArray>()
                .ok_or_else(|| invalid(py, "Arrow Struct physical type differs"))?;
            write_sequence(
                py,
                fields.num_columns(),
                output,
                context,
                visitor,
                |field, output, visitor| {
                    write_value(
                        py,
                        fields.column(field),
                        index,
                        output,
                        context,
                        budget,
                        visitor,
                    )
                },
            )
        }
        DataType::Dictionary(_, _) => {
            let (values, label) = dictionary_label(py, array, index)?;
            write_value(py, values, label, output, context, budget, visitor)
        }
        _ => Err(invalid(py, "unsupported Arrow logical input dtype")),
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
                .downcast_ref::<DictionaryArray<$key>>()
                .ok_or_else(|| invalid(py, "Arrow Dictionary physical key type differs"))?;
            let key = dictionary.keys().value(index);
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
        DataType::Dictionary(key, _) => match key.as_ref() {
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

#[expect(
    clippy::too_many_arguments,
    reason = "checked child range is written synchronously without an owned intermediate value"
)]
fn write_list(
    py: Python<'_>,
    values: &ArrayRef,
    start: usize,
    end: usize,
    output: &mut Vec<u8>,
    context: &Context,
    budget: &IoBudget,
    visitor: &mut dyn CellWriter,
) -> PyResult<()> {
    if start > end || end > values.len() {
        return Err(invalid(
            py,
            "Arrow list offsets are outside the values array",
        ));
    }
    write_sequence(
        py,
        end - start,
        output,
        context,
        visitor,
        |index, output, visitor| {
            write_value(py, values, start + index, output, context, budget, visitor)
        },
    )
}

fn write_sequence(
    py: Python<'_>,
    width: usize,
    output: &mut Vec<u8>,
    context: &Context,
    visitor: &mut dyn CellWriter,
    mut value: impl FnMut(usize, &mut Vec<u8>, &mut dyn CellWriter) -> PyResult<()>,
) -> PyResult<()> {
    visitor.enter_native(py)?;
    let result = (|| {
        writer::append(output, b"(", context.limit).map_err(|error| errors::wire(py, error))?;
        for index in 0..width {
            visitor.push_index(py, index)?;
            if index != 0 {
                writer::append(output, b",", context.limit)
                    .map_err(|error| errors::wire(py, error))?;
            }
            value(index, output, visitor)?;
            visitor.pop_index();
        }
        writer::append(output, b")", context.limit).map_err(|error| errors::wire(py, error))
    })();
    visitor.leave_native();
    result
}
