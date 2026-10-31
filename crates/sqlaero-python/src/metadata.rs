//! Immutable result metadata with native unique keys and lazy Python values.

use crate::{errors, output};
use pyo3::exceptions::{
    PyIndexError, PyKeyError, PyOverflowError, PyTypeError, PyUnicodeEncodeError,
};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PySlice, PyString, PyTuple};
use sqlaero_core::charset::{self, output::OutputCodec};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::memory::{Buffer, Reservation, Scope};
use sqlaero_core::result::{
    keys::{KeyError, Keys, Name},
    text::{Columns as NativeColumns, Label},
};
use std::mem::size_of;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum Attr {
    Index,
    Key,
    Name,
    OriginalName,
    Table,
    OriginalTable,
    Database,
    TypeCode,
    TypeName,
    ColumnLength,
    Precision,
    Scale,
    Unsigned,
    Zerofill,
    FractionalSeconds,
    CollationId,
    CollationName,
    CharsetName,
    BinaryString,
    Enum,
    Set,
    Nullable,
    AutoIncrement,
    PrimaryKey,
    UniqueKey,
    NonUniqueKey,
    Flags,
    Decimals,
    Catalog,
    Names,
    Keys,
    Description,
}
struct ValueCache {
    column: usize,
    attr: Attr,
    value: Py<PyAny>,
}
struct NativeText {
    bytes: Buffer<u8>,
    _owner: Option<Reservation>,
}
struct LabelCache {
    column: usize,
    label: usize,
    value: Arc<NativeText>,
}
struct OwnedNames {
    name: Option<Arc<NativeText>>,
    database: Option<Arc<NativeText>>,
    table: Option<Arc<NativeText>>,
}

/// One execution-time metadata owner. It cannot retain a Cursor, Connection,
/// row buffer, or Python ColumnInfo/Columns object through its value caches.
pub(crate) struct Snapshot {
    raw: Option<Arc<NativeColumns>>,
    codec: OutputCodec,
    strict: bool,
    scope: Option<Arc<Scope>>,
    labels: Mutex<Buffer<LabelCache>>,
    values: Mutex<Buffer<ValueCache>>,
    keys: OnceLock<Keys>,
    key_preparation: Mutex<()>,
    _owner: Option<Reservation>,
}
fn storage<T>(scope: Option<&Arc<Scope>>) -> Buffer<T> {
    scope.map_or_else(Buffer::new, Buffer::tracked)
}
fn internal(py: Python<'_>, message: &'static str) -> PyErr {
    errors::client(py, ClientErrorCode::InternalState, message)
}
fn lock<'a, T>(py: Python<'_>, value: &'a Mutex<T>) -> PyResult<MutexGuard<'a, T>> {
    match value.lock() {
        Ok(value) => Ok(value),
        Err(error) => {
            drop(error.into_inner());
            Err(internal(
                py,
                "immutable metadata cache was invalidated by a native failure",
            ))
        }
    }
}
fn integer<'py>(py: Python<'py>, value: u64) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: attached final constructor; it returns a new exact int reference
    // or the original Python allocation error, with no intermediate string.
    unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromUnsignedLongLong(value)) }
}
fn boolean<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    match value.into_pyobject(py) {
        Ok(value) => value.to_owned().into_any(),
        Err(never) => match never {},
    }
}
fn wtf8<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyString>> {
    // SAFETY: the initialized lossless UTF-8/WTF-8 slice stays alive for the
    // copying decoder. surrogatepass preserves recovered metadata bytes.
    unsafe {
        Bound::<PyAny>::from_owned_ptr_or_err(
            py,
            pyo3::ffi::PyUnicode_DecodeUTF8(
                data.as_ptr().cast(),
                data.len() as isize,
                c"surrogatepass".as_ptr(),
            ),
        )?
        .cast_into()
        .map_err(Into::into)
    }
}
fn text_bytes(value: &Option<Arc<NativeText>>) -> &[u8] {
    value.as_ref().map_or(b"", |value| value.bytes.as_slice())
}

impl Snapshot {
    /// Reuse only a previously prepared unique key for optional failure detail.
    /// This never prepares all keys or creates a Columns/ColumnInfo object.
    pub(crate) fn diagnostic_key(&self, py: Python<'_>, index: usize) -> Option<String> {
        let bytes = self.keys.get()?.get(index)?;
        wtf8(py, bytes)
            .and_then(|text| text.repr())
            .and_then(|text| text.extract())
            .ok()
    }
    pub(crate) fn new(
        py: Python<'_>,
        raw: Option<Arc<NativeColumns>>,
        codec: OutputCodec,
        strict: bool,
    ) -> PyResult<Arc<Self>> {
        let scope = raw.as_ref().map(|raw| Arc::clone(raw.scope()));
        let owner = scope
            .as_ref()
            .map(|scope| scope.charge(size_of::<Self>() + 2 * size_of::<usize>()))
            .transpose()
            .map_err(|error| output::memory(py, error))?;
        Ok(Arc::new(Self {
            raw,
            codec,
            strict,
            labels: Mutex::new(storage(scope.as_ref())),
            values: Mutex::new(storage(scope.as_ref())),
            keys: OnceLock::new(),
            key_preparation: Mutex::new(()),
            scope,
            _owner: owner,
        }))
    }
    fn len(&self) -> usize {
        self.raw.as_ref().map_or(0, |raw| raw.len())
    }
    fn label(
        &self,
        py: Python<'_>,
        index: usize,
        label: Label,
    ) -> PyResult<Option<Arc<NativeText>>> {
        let raw = self
            .raw
            .as_ref()
            .and_then(|raw| raw.label(index, label))
            .ok_or_else(|| internal(py, "column label ownership is unavailable"))?;
        if raw.is_empty() {
            return Ok(None);
        }
        let key = (index, label as usize);
        {
            let cache = lock(py, &self.labels)?;
            if let Ok(position) =
                cache.binary_search_by_key(&key, |entry| (entry.column, entry.label))
            {
                return Ok(Some(Arc::clone(&cache[position].value)));
            }
        }
        let points = output::points(py, raw, self.codec, self.strict, self.scope.as_ref())?;
        let mut bytes = storage(self.scope.as_ref());
        for point in points.iter().copied() {
            if let Some(point) = char::from_u32(point) {
                let mut array = [0; 4];
                bytes
                    .extend_from_slice(point.encode_utf8(&mut array).as_bytes())
                    .map_err(|error| output::memory(py, error))?;
            } else if (0xd800..=0xdfff).contains(&point) {
                bytes
                    .extend_from_slice(&[
                        0xe0 | (point >> 12) as u8,
                        0x80 | ((point >> 6) & 63) as u8,
                        0x80 | (point & 63) as u8,
                    ])
                    .map_err(|error| output::memory(py, error))?;
            } else {
                return Err(internal(
                    py,
                    "metadata decoder returned an invalid Unicode point",
                ));
            }
        }
        let owner = self
            .scope
            .as_ref()
            .map(|scope| scope.charge(size_of::<NativeText>() + 2 * size_of::<usize>()))
            .transpose()
            .map_err(|error| output::memory(py, error))?;
        let value = Arc::new(NativeText {
            bytes,
            _owner: owner,
        });
        let inserted = {
            let mut cache = lock(py, &self.labels)?;
            match cache.binary_search_by_key(&key, |entry| (entry.column, entry.label)) {
                Ok(position) => return Ok(Some(Arc::clone(&cache[position].value))),
                Err(position) => cache.insert(
                    position,
                    LabelCache {
                        column: index,
                        label: label as usize,
                        value: Arc::clone(&value),
                    },
                ),
            }
        };
        inserted.map_err(|error| output::memory(py, error))?;
        Ok(Some(value))
    }
    fn prepared_keys(&self, py: Python<'_>) -> PyResult<&Keys> {
        if let Some(keys) = self.keys.get() {
            return Ok(keys);
        }
        let _guard = self.key_preparation.try_lock().map_err(|_| {
            errors::client(
                py,
                ClientErrorCode::InterfaceBusy,
                "this result's column keys are already being prepared",
            )
        })?;
        if let Some(keys) = self.keys.get() {
            return Ok(keys);
        }
        let mut owned = storage(self.scope.as_ref());
        owned
            .reserve(self.len())
            .map_err(|error| output::memory(py, error))?;
        for index in 0..self.len() {
            owned
                .push(OwnedNames {
                    name: self.label(py, index, Label::Name)?,
                    database: self.label(py, index, Label::Database)?,
                    table: self.label(py, index, Label::Table)?,
                })
                .map_err(|error| output::memory(py, error))?;
            if index % 256 == 0 {
                py.check_signals()?;
            }
        }
        let mut names = storage(self.scope.as_ref());
        names
            .reserve(self.len())
            .map_err(|error| output::memory(py, error))?;
        for item in owned.iter() {
            names
                .push(Name {
                    label: text_bytes(&item.name),
                    database: text_bytes(&item.database),
                    table: text_bytes(&item.table),
                })
                .map_err(|error| output::memory(py, error))?;
        }
        let scope = self.scope.as_ref();
        let keys = py
            .detach(|| Keys::new(&names, scope))
            .map_err(|error| match error {
                KeyError::Memory(error) => output::memory(py, error),
                KeyError::Invariant(message) => internal(py, message),
            })?;
        if self.keys.set(keys).is_err() {
            return Err(internal(
                py,
                "column keys were concurrently published twice",
            ));
        }
        self.keys
            .get()
            .ok_or_else(|| internal(py, "complete column keys were not published"))
    }
    fn value<'py>(&self, py: Python<'py>, index: usize, attr: Attr) -> PyResult<Bound<'py, PyAny>> {
        let key = (index, attr);
        {
            let cache = lock(py, &self.values)?;
            if let Ok(position) =
                cache.binary_search_by_key(&key, |entry| (entry.column, entry.attr))
            {
                return Ok(cache[position].value.clone_ref(py).into_bound(py));
            }
        }
        let value = self.uncached(py, index, attr)?;
        let inserted = {
            let mut cache = lock(py, &self.values)?;
            match cache.binary_search_by_key(&key, |entry| (entry.column, entry.attr)) {
                Ok(position) => return Ok(cache[position].value.clone_ref(py).into_bound(py)),
                Err(position) => cache.insert(
                    position,
                    ValueCache {
                        column: index,
                        attr,
                        value: value.clone().unbind(),
                    },
                ),
            }
        };
        inserted.map_err(|error| output::memory(py, error))?;
        Ok(value)
    }
    fn uncached<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        attr: Attr,
    ) -> PyResult<Bound<'py, PyAny>> {
        if matches!(attr, Attr::Names | Attr::Keys | Attr::Description) {
            if attr == Attr::Description && self.len() == 0 {
                return Ok(py.None().into_bound(py));
            }
            let mut values = Vec::new();
            values
                .try_reserve(self.len())
                .map_err(|_| errors::memory_error(py))?;
            for index in 0..self.len() {
                let value = match attr {
                    Attr::Names => self.value(py, index, Attr::Name)?,
                    Attr::Keys => self.value(py, index, Attr::Key)?,
                    _ => output::tuple_from_ready(
                        py,
                        [
                            self.value(py, index, Attr::Name)?,
                            self.value(py, index, Attr::TypeCode)?,
                            py.None().into_bound(py),
                            py.None().into_bound(py),
                            self.value(py, index, Attr::Precision)?,
                            self.value(py, index, Attr::Scale)?,
                            self.value(py, index, Attr::Nullable)?,
                        ],
                    )?
                    .into_any(),
                };
                values.push(value.unbind());
            }
            return Ok(output::tuple_from_ready_vec(py, values)?.into_any());
        }
        let column = self
            .raw
            .as_ref()
            .and_then(|raw| raw.get(index))
            .ok_or_else(|| internal(py, "ColumnInfo has no matching native column"))?;
        let label = match attr {
            Attr::Name => Some(Label::Name),
            Attr::OriginalName => Some(Label::OriginalName),
            Attr::Table => Some(Label::Table),
            Attr::OriginalTable => Some(Label::OriginalTable),
            Attr::Database => Some(Label::Database),
            Attr::Catalog => Some(Label::Catalog),
            _ => None,
        };
        if let Some(label) = label {
            let value = self.label(py, index, label)?;
            if value.is_none() && attr != Attr::Name {
                return Ok(py.None().into_bound(py));
            }
            return wtf8(py, text_bytes(&value)).map(Bound::into_any);
        }
        let optional = |value: Option<u32>| -> PyResult<Bound<'py, PyAny>> {
            match value {
                Some(value) => integer(py, u64::from(value)),
                None => Ok(py.None().into_bound(py)),
            }
        };
        let literal = |value: Option<&str>| -> PyResult<Bound<'py, PyAny>> {
            match value {
                Some(value) => wtf8(py, value.as_bytes()).map(Bound::into_any),
                None => Ok(py.None().into_bound(py)),
            }
        };
        match attr {
            Attr::Index => integer(py, index as u64),
            Attr::Key => wtf8(
                py,
                self.prepared_keys(py)?
                    .get(index)
                    .ok_or_else(|| internal(py, "column key is unavailable"))?,
            )
            .map(Bound::into_any),
            Attr::TypeCode => integer(py, u64::from(column.type_code)),
            Attr::TypeName => literal(column.type_name()),
            Attr::ColumnLength => integer(py, u64::from(column.column_length)),
            Attr::Precision => optional(column.precision()),
            Attr::Scale => optional(column.scale().map(u32::from)),
            Attr::Unsigned => Ok(boolean(py, column.is_unsigned())),
            Attr::Zerofill => Ok(boolean(py, column.is_zerofill())),
            Attr::FractionalSeconds => {
                optional(column.fractional_seconds_precision().map(u32::from))
            }
            Attr::CollationId => integer(py, u64::from(column.collation_id)),
            Attr::CollationName => {
                literal(charset::by_id(column.collation_id).map(|(collation, _)| collation.name))
            }
            Attr::CharsetName => {
                literal(charset::by_id(column.collation_id).map(|(_, charset)| charset.name))
            }
            Attr::BinaryString => Ok(boolean(py, column.is_binary_string())),
            Attr::Enum => Ok(boolean(py, column.is_enum())),
            Attr::Set => Ok(boolean(py, column.is_set())),
            Attr::Nullable => Ok(boolean(py, column.flags & 1 == 0)),
            Attr::AutoIncrement => Ok(boolean(py, column.flags & 512 != 0)),
            Attr::PrimaryKey => Ok(boolean(py, column.flags & 2 != 0)),
            Attr::UniqueKey => Ok(boolean(py, column.flags & 4 != 0)),
            Attr::NonUniqueKey => Ok(boolean(py, column.flags & 8 != 0)),
            Attr::Flags => integer(py, u64::from(column.flags)),
            Attr::Decimals => integer(py, u64::from(column.decimals)),
            _ => Err(internal(py, "invalid metadata attribute action")),
        }
    }
    pub(crate) fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        self.value(py, usize::MAX, Attr::Names)?
            .cast_into()
            .map_err(Into::into)
    }
    pub(crate) fn description<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, usize::MAX, Attr::Description)
    }
}

/// Immutable metadata for one result column. Unknown optional facts are None;
/// properties never query the server or inspect all result values.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct ColumnInfo {
    snapshot: Arc<Snapshot>,
    index: usize,
}
struct ItemCache {
    index: usize,
    value: Py<ColumnInfo>,
}
/// Immutable ordered metadata with integer/slice access and exact unique-key
/// lookup. Its cached items own only their native metadata snapshot.
#[pyclass(frozen, module = "sqlaero")]
pub(crate) struct Columns {
    snapshot: Arc<Snapshot>,
    items: Mutex<Buffer<ItemCache>>,
}
impl Columns {
    pub(crate) fn new(snapshot: Arc<Snapshot>) -> Self {
        let items = storage(snapshot.scope.as_ref());
        Self {
            snapshot,
            items: Mutex::new(items),
        }
    }
    fn item(&self, py: Python<'_>, index: usize) -> PyResult<Py<ColumnInfo>> {
        {
            let items = lock(py, &self.items)?;
            if let Ok(position) = items.binary_search_by_key(&index, |item| item.index) {
                return Ok(items[position].value.clone_ref(py));
            }
        }
        let value = Py::new(
            py,
            ColumnInfo {
                snapshot: Arc::clone(&self.snapshot),
                index,
            },
        )?;
        let inserted = {
            let mut items = lock(py, &self.items)?;
            match items.binary_search_by_key(&index, |item| item.index) {
                Ok(position) => return Ok(items[position].value.clone_ref(py)),
                Err(position) => items.insert(
                    position,
                    ItemCache {
                        index,
                        value: value.clone_ref(py),
                    },
                ),
            }
        };
        inserted.map_err(|error| output::memory(py, error))?;
        Ok(value)
    }
}
#[pymethods]
impl Columns {
    fn __len__(&self) -> usize {
        self.snapshot.len()
    }
    fn __repr__(&self) -> String {
        format!("Columns(len={})", self.snapshot.len())
    }
    /// Actual ordered query labels, preserving AS and duplicates. Cached once;
    /// reading this property does not derive global unique lookup keys.
    #[getter]
    fn names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        self.snapshot.names(py)
    }
    /// Unique exact keys in column order. Generated once without changing names
    /// or source labels. Dots are access labels, not reversible SQL paths.
    #[getter]
    fn keys<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        self.snapshot
            .value(py, usize::MAX, Attr::Keys)?
            .cast_into()
            .map_err(Into::into)
    }
    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        key: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Ok(slice) = key.cast::<PySlice>() {
            let slice = slice.indices(self.snapshot.len() as isize)?;
            let mut values = Vec::new();
            values
                .try_reserve(slice.slicelength)
                .map_err(|_| errors::memory_error(py))?;
            let mut index = slice.start;
            for offset in 0..slice.slicelength {
                values.push(self.item(py, index as usize)?.into_any());
                if offset + 1 < slice.slicelength {
                    index += slice.step;
                }
            }
            return Ok(output::tuple_from_ready_vec(py, values)?.into_any());
        }
        if let Ok(key) = key.cast::<PyString>() {
            let keys = self.snapshot.prepared_keys(py)?;
            let encoded;
            let bytes = match key.to_str() {
                Ok(value) => value.as_bytes(),
                Err(error) if error.is_instance_of::<PyUnicodeEncodeError>(py) => {
                    // SAFETY: the attached encoder copies this live Python key
                    // and returns a new owned bytes reference, or its exception.
                    encoded = unsafe {
                        Bound::<PyAny>::from_owned_ptr_or_err(
                            py,
                            pyo3::ffi::PyUnicode_AsEncodedString(
                                key.as_ptr(),
                                c"utf-8".as_ptr(),
                                c"surrogatepass".as_ptr(),
                            ),
                        )?
                    };
                    encoded.cast::<pyo3::types::PyBytes>()?.as_bytes()
                }
                Err(error) => return Err(error),
            };
            let index = keys
                .find(bytes)
                .ok_or_else(|| PyKeyError::new_err(key.clone().unbind()))?;
            return Ok(self.item(py, index)?.into_bound(py).into_any());
        }
        if key.is_instance_of::<PyInt>() {
            let mut index = match key.extract::<isize>() {
                Ok(value) => value,
                Err(error) if error.is_instance_of::<PyOverflowError>(py) => {
                    return Err(crate::errors::builtin_message::<PyIndexError>(
                        py,
                        "column index is out of range",
                    ));
                }
                Err(error) => return Err(error),
            };
            if index < 0 {
                index += self.snapshot.len() as isize;
            }
            if index < 0 || index as usize >= self.snapshot.len() {
                return Err(crate::errors::builtin_message::<PyIndexError>(
                    py,
                    "column index is out of range",
                ));
            }
            return Ok(self.item(py, index as usize)?.into_bound(py).into_any());
        }
        Err(crate::errors::builtin_message::<PyTypeError>(
            py,
            "Columns indexes must be integers, exact string keys or slices",
        ))
    }
}
macro_rules! attributes {
    ($(($name:ident, $attr:ident, $doc:literal)),* $(,)?) => {
        #[pymethods]
        impl ColumnInfo {
            $(#[getter] #[doc=$doc] fn $name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> { self.snapshot.value(py, self.index, Attr::$attr) })*
            fn __repr__(&self) -> String { format!("ColumnInfo(index={})", self.index) }
        }
    }
}
attributes! {
    (index, Index, "Zero-based original position in this immutable query result."),
    (key, Key, "Unique exact lookup key. First access derives all snapshot keys."),
    (name, Name, "Actual query-visible label including AS; duplicates and empty labels remain."),
    (original_name, OriginalName, "Original source column name, or None when unavailable."),
    (table, Table, "Query-visible source table or alias, or None when unavailable."),
    (original_table, OriginalTable, "Original physical table name, or None when unavailable."),
    (database, Database, "Protocol source schema/database, or None when unavailable."),
    (type_code, TypeCode, "Actual integer MySQL protocol type code, including unknown codes."),
    (type_name, TypeName, "Known protocol type name, or None; this does not reconstruct original DDL."),
    (column_length, ColumnLength, "Server-declared display bytes, not actual length, characters or storage size."),
    (precision, Precision, "Reliable Decimal precision, or None; integer display width is not precision."),
    (scale, Scale, "Applicable numeric fractional digits, or None for unknown/special markers."),
    (is_unsigned, Unsigned, "Whether the actual unsigned flag applies to this numeric type."),
    (is_zerofill, Zerofill, "Whether the actual legacy zero-fill flag applies to this numeric type."),
    (fractional_seconds_precision, FractionalSeconds, "Declared temporal fractional digits 0-6, or None."),
    (collation_id, CollationId, "Complete actual protocol character_set/collation ID, including unknown IDs."),
    (collation_name, CollationName, "Catalog collation name for the actual ID, or None when unrecognized."),
    (charset_name, CharsetName, "Catalog charset for the actual collation, or None when unrecognized."),
    (is_binary_string, BinaryString, "Whether a string family uses binary encoding; textual _bin collations stay text."),
    (is_enum, Enum, "Whether actual type/flags report ENUM; values need not be single characters."),
    (is_set, Set, "Whether actual type/flags report SET; schema members are not queried."),
    (is_nullable, Nullable, "Inverse of actual NOT_NULL_FLAG; no value scan is performed."),
    (is_auto_increment, AutoIncrement, "Whether the actual protocol reports AUTO_INCREMENT_FLAG."),
    (is_primary_key_part, PrimaryKey, "Whether the protocol reports primary-key membership."),
    (is_unique_key_part, UniqueKey, "Reported unique-index membership; this column alone need not be unique."),
    (is_non_unique_key_part, NonUniqueKey, "Whether the protocol reports ordinary-index membership."),
    (flags, Flags, "Actual raw MySQL flag bits without inferred schema information."),
    (decimals, Decimals, "Actual raw decimals/fractional-second byte, including special markers."),
    (catalog, Catalog, "Actual validated protocol catalog, def."),
}
