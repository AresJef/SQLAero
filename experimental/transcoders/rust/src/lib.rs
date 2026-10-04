//! Independent SQL and JSON conversion with owned buffers and bounded recursion.
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyModule, PyString, PyTuple, PyType,
};
use std::collections::HashSet;
mod dispatch;
const DEPTH: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;
#[pyclass]
struct Registry {
    objstr: Vec<Py<PyAny>>,
    rawtext: Vec<Py<PyAny>>,
    sqlfunction: Vec<Py<PyAny>>,
    sqlinterval: Vec<Py<PyAny>>,
}
/// Construct a value error without exposing Rust panics across the Python boundary.
fn error(s: impl Into<String>) -> PyErr {
    PyValueError::new_err(s.into())
}
// Central declarations for reviewed extension types. New types need a handler and vectors.
const KNOWN_TYPES: &[(&str, &str)] = &[
    ("builtins", "bytearray"),
    ("builtins", "memoryview"),
    ("builtins", "set"),
    ("builtins", "frozenset"),
    ("builtins", "range"),
    ("builtins", "dict_keys"),
    ("builtins", "dict_values"),
    ("builtins", "dict_items"),
    ("decimal", "Decimal"),
    ("datetime", "datetime"),
    ("datetime", "date"),
    ("datetime", "time"),
    ("datetime", "timedelta"),
    ("time", "struct_time"),
    ("numpy", "generic"),
    ("numpy", "ndarray"),
    ("numpy", "record"),
    ("numpy", "str_"),
    ("pandas", "DataFrame"),
    ("pandas", "Series"),
    ("pandas", "Index"),
    ("pandas", "NaTType"),
    ("cytimes", "Pddt"),
    ("uuid", "UUID"),
    ("enum", "Enum"),
];
/// Look up a reviewed type once-cached at import and apply its declared subclass rule.
fn is_type(o: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<bool> {
    let cache = o
        .py()
        .import("sqlaero_rust_kernel")?
        .getattr("_type_cache")?;
    let cache = cache.cast::<PyDict>()?;
    match cache.get_item(format!("{module}.{name}"))? {
        Some(t) => o.is_instance(&t),
        None => Ok(false),
    }
}
/// Compare a reviewed cached class identity without broadening its JSON subclass domain.
fn exact_type(o: &Bound<'_, PyAny>, module: &str, name: &str) -> PyResult<bool> {
    let cache = o
        .py()
        .import("sqlaero_rust_kernel")?
        .getattr("_type_cache")?;
    let cache = cache.cast::<PyDict>()?;
    Ok(cache
        .get_item(format!("{module}.{name}"))?
        .is_some_and(|t| o.get_type().is(&t)))
}
/// Recognize only classes explicitly registered by the trusted ABI bridge.
fn trusted(o: &Bound<'_, PyAny>, types: &[Py<PyAny>]) -> PyResult<bool> {
    for t in types {
        if o.is_instance(t.bind(o.py()))? {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Borrow the module-local trusted class registry for this conversion.
fn registry<'py>(py: Python<'py>) -> PyResult<PyRef<'py, Registry>> {
    py.import("sqlaero_rust_kernel")?
        .getattr("_registry")?
        .extract()
        .map_err(Into::into)
}
/// Register explicit SQL trust boundaries. Ordinary objects are never trusted by duck typing.
#[pyfunction(signature=(*, objstr=None, rawtext=None, sqlfunction=None, sqlinterval=None), text_signature="(*, objstr=(), rawtext=(), sqlfunction=(), sqlinterval=())")]
fn configure_types(
    py: Python<'_>,
    objstr: Option<&Bound<'_, PyTuple>>,
    rawtext: Option<&Bound<'_, PyTuple>>,
    sqlfunction: Option<&Bound<'_, PyTuple>>,
    sqlinterval: Option<&Bound<'_, PyTuple>>,
) -> PyResult<()> {
    /// Validate and snapshot trusted registration tuples before replacing module state.
    fn collect(t: Option<&Bound<'_, PyTuple>>) -> PyResult<Vec<Py<PyAny>>> {
        let Some(t) = t else { return Ok(vec![]) };
        t.iter()
            .map(|x| {
                if !x.is_instance_of::<PyType>() {
                    return Err(PyTypeError::new_err(
                        "registered trust entries must be classes",
                    ));
                }
                Ok(x.unbind())
            })
            .collect()
    }
    let r = Registry {
        objstr: collect(objstr)?,
        rawtext: collect(rawtext)?,
        sqlfunction: collect(sqlfunction)?,
        sqlinterval: collect(sqlinterval)?,
    };
    py.import("sqlaero_rust_kernel")?
        .setattr("_registry", Py::new(py, r)?)?;
    Ok(())
}
/// Copy Python Unicode losslessly through UTF-8 surrogatepass for SQL transport.
fn utf8_sql(o: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(o.py()
        .get_type::<PyString>()
        .getattr("encode")?
        .call1((o, "utf-8", "surrogatepass"))?
        .cast::<PyBytes>()?
        .as_bytes()
        .to_vec())
}
/// Construct Python Unicode from lossless SQL UTF-8 including lone surrogates.
fn sql_text<'py>(py: Python<'py>, b: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    Ok(PyBytes::new(py, b).call_method1("decode", ("utf-8", "surrogatepass"))?)
}
/// Write MySQL backslash-mode string escaping, optionally adding the binary introducer.
fn quote(s: &[u8], binary: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 10);
    if binary {
        out.extend_from_slice(b"_binary");
    }
    out.push(b'\'');
    for &x in s {
        match x {
            0 => out.extend_from_slice(b"\\0"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            26 => out.extend_from_slice(b"\\Z"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' => out.extend_from_slice(b"\\'"),
            b'"' => out.extend_from_slice(b"\\\""),
            _ => out.push(x),
        }
    }
    out.push(b'\'');
    out
}
/// Format finite f64 values using the reviewed shortest formatter; SQL nonfinite becomes NULL.
fn float_text(f: f64) -> String {
    if !f.is_finite() {
        "NULL".into()
    } else {
        zmij::Buffer::new().format_finite(f).to_owned()
    }
}
/// Preserve f32 shortest formatting without widening finite array values to f64.
fn float32_text(f: f32) -> String {
    if !f.is_finite() {
        "NULL".into()
    } else {
        zmij::Buffer::new().format_finite(f).to_owned()
    }
}
/// Write row punctuation according to container structure, preserving item order.
fn join_row(items: Vec<Vec<u8>>, nested: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if !nested {
        out.push(b'(');
    }
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        out.extend(s);
    }
    if !nested {
        out.push(b')');
    }
    out
}
/// Extract a checked temporal integer component through the Python object API.
fn attr_i(o: &Bound<'_, PyAny>, name: &str) -> PyResult<i64> {
    o.getattr(name)?.extract()
}
/// Render local time components with optional six-digit microseconds.
fn time_fields(o: &Bound<'_, PyAny>) -> PyResult<String> {
    let mut s = format!(
        "{:02}:{:02}:{:02}",
        attr_i(o, "hour")?,
        attr_i(o, "minute")?,
        attr_i(o, "second")?
    );
    let us = attr_i(o, "microsecond")?;
    if us != 0 {
        s.push_str(&format!(".{us:06}"));
    }
    Ok(s)
}
/// Render local calendar fields with fixed-width ISO date punctuation.
fn date_fields(o: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(format!(
        "{:04}-{:02}-{:02}",
        attr_i(o, "year")?,
        attr_i(o, "month")?,
        attr_i(o, "day")?
    ))
}
/// Render a checked wide microsecond duration, including negative and multi-day values.
fn duration(us: i128) -> Vec<u8> {
    let neg = us < 0;
    let v = us.unsigned_abs();
    let sec = v / 1_000_000;
    let mut s = format!(
        "{}{:02}:{:02}:{:02}",
        if neg { "-" } else { "" },
        sec / 3600,
        sec / 60 % 60,
        sec % 60
    );
    if v % 1_000_000 != 0 {
        s.push_str(&format!(".{:06}", v % 1_000_000));
    }
    quote(s.as_bytes(), false)
}
// Proleptic Gregorian calendar arithmetic, implemented from calendar cycle identities.
/// Convert epoch-day counts to proleptic Gregorian fields using calendar cycle arithmetic.
fn civil(days: i128) -> (i128, i128, i128) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}
/// Convert signed NumPy ticks with wide arithmetic, unit checks, NaT handling and year bounds.
fn np_time(v: i64, unit: &str, kind: &str) -> PyResult<Vec<u8>> {
    if v == i64::MIN {
        return Ok(b"NULL".to_vec());
    }
    let n = v as i128;
    if kind == "m" {
        let mul = match unit {
            "ns" => 0,
            "us" => 1,
            "ms" => 1000,
            "s" => 1_000_000,
            "m" => 60_000_000,
            "h" => 3_600_000_000,
            "D" => 86_400_000_000,
            "W" => 604_800_000_000,
            "M" => 2_629_746_000_000,
            "Y" => 31_556_952_000_000,
            _ => return Err(error("unsupported timedelta64 unit")),
        };
        return Ok(duration(if unit == "ns" {
            n.div_euclid(1000)
        } else {
            n * mul
        }));
    }
    if unit == "Y" || unit == "M" {
        let months = if unit == "Y" { n * 12 } else { n };
        let y = 1970 + months.div_euclid(12);
        let m = months.rem_euclid(12) + 1;
        if !(1..=9999).contains(&y) {
            return Err(error("datetime64 year out of range"));
        }
        return Ok(quote(
            format!("{y:04}-{m:02}-01 00:00:00").as_bytes(),
            false,
        ));
    }
    let us = match unit {
        "ns" => n.div_euclid(1000),
        "us" => n,
        "ms" => n * 1000,
        "s" => n * 1_000_000,
        "m" => n * 60_000_000,
        "h" => n * 3_600_000_000,
        "D" => n * 86_400_000_000,
        "W" => n * 604_800_000_000,
        _ => return Err(error("unsupported datetime64 unit")),
    };
    let days = us.div_euclid(86_400_000_000);
    let rest = us.rem_euclid(86_400_000_000);
    let (y, m, d) = civil(days);
    if !(1..=9999).contains(&y) {
        return Err(error("datetime64 year out of range"));
    }
    let mut s = format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rest / 3_600_000_000,
        rest / 60_000_000 % 60,
        rest / 1_000_000 % 60
    );
    if rest % 1_000_000 != 0 {
        s.push_str(&format!(".{:06}", rest % 1_000_000));
    }
    Ok(quote(s.as_bytes(), false))
}
/// Read the base NumPy temporal unit, preserving the frozen multiplier-ignoring policy.
fn np_unit(o: &Bound<'_, PyAny>) -> PyResult<String> {
    let dtype = o.getattr("dtype")?;
    let info = o
        .py()
        .import("numpy")?
        .getattr("datetime_data")?
        .call1((dtype,))?;
    let unit: String = info.get_item(0)?.extract()?;
    // Known baseline semantic bug: dtype multiplier is ignored until separately approved.
    Ok(unit)
}
/// Reject multiplied JSON temporal metadata until its independent acceptance policy is validated.
fn json_np_unit(o: &Bound<'_, PyAny>) -> PyResult<String> {
    let info = o
        .py()
        .import("numpy")?
        .getattr("datetime_data")?
        .call1((o.getattr("dtype")?,))?;
    let count: i64 = info.get_item(1)?.extract()?;
    if count != 1 {
        return Err(PyTypeError::new_err(
            "multiplied NumPy JSON temporal units are unvalidated",
        ));
    }
    np_unit(o)
}
/// Snapshot only reviewed SQL container families, treating dictionaries as ordered values.
fn container(o: &Bound<'_, PyAny>) -> PyResult<Option<Vec<Py<PyAny>>>> {
    let py = o.py();
    if let Ok(d) = o.cast::<PyDict>() {
        return Ok(Some(d.values().iter().map(|v| v.unbind()).collect()));
    }
    if is_type(o, "time", "struct_time")? {
        return Ok(None);
    }
    if let Ok(list) = o.cast::<PyList>() {
        if o.is_exact_instance_of::<PyList>() {
            return Ok(Some(list.iter().map(|v| v.unbind()).collect()));
        }
        let normalized = py.get_type::<PyList>().call1((o,))?;
        return Ok(Some(
            normalized
                .cast::<PyList>()?
                .iter()
                .map(|v| v.unbind())
                .collect(),
        ));
    }
    if let Ok(tuple) = o.cast::<PyTuple>() {
        if o.is_exact_instance_of::<PyTuple>() {
            return Ok(Some(tuple.iter().map(|v| v.unbind()).collect()));
        }
        let normalized = py.get_type::<PyTuple>().call1((o,))?;
        return Ok(Some(
            normalized
                .cast::<PyTuple>()?
                .iter()
                .map(|v| v.unbind())
                .collect(),
        ));
    }
    if exact_type(o, "builtins", "dict_items")? {
        let d = py.import("builtins")?.getattr("dict")?.call1((o,))?;
        return container(&d);
    }
    if o.is_instance_of::<PyList>()
        || o.is_instance_of::<PyTuple>()
        || (is_type(o, "builtins", "set")?
            || is_type(o, "builtins", "frozenset")?
            || is_type(o, "builtins", "range")?
            || exact_type(o, "builtins", "dict_keys")?
            || exact_type(o, "builtins", "dict_values")?)
        || is_type(o, "numpy", "record")?
    {
        return Ok(Some(
            o.try_iter()?
                .map(|x| x.map(|v| v.unbind()))
                .collect::<PyResult<_>>()?,
        ));
    }
    Ok(None)
}
/// Enforce the shared bounded recursion policy before descending.
fn check_depth(depth: usize) -> PyResult<()> {
    if depth > DEPTH {
        Err(error("conversion nesting exceeds 128"))
    } else {
        Ok(())
    }
}
/// Snapshot array bytes before checked endian-aware native dtype loops or safe object indexing.
fn ndarray_rows(
    o: &Bound<'_, PyAny>,
    depth: usize,
    seen: &mut HashSet<usize>,
) -> PyResult<Vec<Vec<Vec<u8>>>> {
    check_depth(depth)?;
    let ndim: usize = o.getattr("ndim")?.extract()?;
    if ndim != 1 && ndim != 2 {
        return Err(error("SQL ndarray requires one or two dimensions"));
    }
    let shape: Vec<usize> = o.getattr("shape")?.extract()?;
    if shape.len() != ndim {
        return Err(error("ndarray shape rank mismatch"));
    }
    let rows = if ndim == 1 { 1 } else { shape[0] };
    let cols = if ndim == 1 { shape[0] } else { shape[1] };
    let count = rows
        .checked_mul(cols)
        .ok_or_else(|| error("array shape overflow"))?;
    let dtype = o.getattr("dtype")?;
    let kind: String = dtype.getattr("kind")?.extract()?;
    let width: usize = dtype.getattr("itemsize")?.extract()?;
    if !matches!(
        kind.as_str(),
        "b" | "i" | "u" | "f" | "m" | "M" | "O" | "S" | "U"
    ) {
        return Err(PyTypeError::new_err("unsupported ndarray dtype"));
    }
    if count > MAX_BYTES / 8 {
        return Err(error("array exceeds element budget"));
    }
    if count
        .checked_mul(width)
        .ok_or_else(|| error("array size overflow"))?
        > MAX_BYTES
    {
        return Err(error("array exceeds byte budget"));
    }
    if count == 0 {
        return Ok(if ndim == 1 { vec![vec![]] } else { vec![] });
    }
    if !matches!(kind.as_str(), "O" | "S" | "U")
        && (!matches!(width, 1 | 2 | 4 | 8)
            || (kind == "f" && !matches!(width, 2 | 4 | 8))
            || ((kind == "m" || kind == "M") && width != 8)
            || (kind == "b" && width != 1))
    {
        return Err(error("invalid numeric dtype itemsize"));
    }
    let mut result = vec![Vec::with_capacity(cols); rows];
    if matches!(kind.as_str(), "O" | "S" | "U") {
        for (r, row) in result.iter_mut().enumerate() {
            for c in 0..cols {
                let val = if ndim == 1 {
                    o.get_item(c)?
                } else {
                    o.get_item((r, c))?
                };
                row.push(scalar(&val, depth + 1, seen)?);
            }
        }
        return Ok(result);
    }
    let raw = o.call_method1("tobytes", ("C",))?;
    let bytes = raw.cast::<PyBytes>()?.as_bytes();
    if bytes.len() != count * width {
        return Err(error("ndarray byte length mismatch"));
    }
    let order: String = dtype.getattr("byteorder")?.extract()?;
    let le = order == "<" || ((order == "=" || order == "|") && cfg!(target_endian = "little"));
    let unit = if kind == "M" || kind == "m" {
        np_unit(o)?
    } else {
        String::new()
    };
    for (i, b) in bytes.chunks_exact(width).enumerate() {
        let mut arr = [0u8; 8];
        if width > 8 {
            return Err(error("unsupported numeric itemsize"));
        }
        if le {
            arr[..width].copy_from_slice(b);
        } else {
            for (j, v) in b.iter().rev().enumerate() {
                arr[j] = *v;
            }
        }
        let u = u64::from_le_bytes(arr);
        let s = match kind.as_str() {
            "b" => {
                if u == 0 {
                    "0".into()
                } else {
                    "1".into()
                }
            }
            "u" => u.to_string(),
            "i" => {
                let shift = 64 - width * 8;
                ((u << shift) as i64 >> shift).to_string()
            }
            "f" => match width {
                8 => float_text(f64::from_bits(u)),
                4 => float32_text(f32::from_bits(u as u32)),
                2 => float32_text(half_float(u as u16)),
                _ => return Err(error("unsupported floating itemsize")),
            },
            "m" | "M" => {
                result[i / cols].push(np_time(u as i64, &unit, &kind)?);
                continue;
            }
            _ => unreachable!(),
        };
        result[i / cols].push(s.into_bytes());
    }
    Ok(result)
}
/// Convert binary16 sign, exponent and fraction into an exactly representable f32 value.
fn half_float(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (bits >> 10) & 31;
    let frac = bits & 1023;
    if exp == 31 {
        if frac == 0 {
            sign * f32::INFINITY
        } else {
            f32::NAN
        }
    } else if exp == 0 {
        sign * (frac as f32) * 2f32.powi(-24)
    } else {
        sign * (1.0 + frac as f32 / 1024.0) * 2f32.powi(exp as i32 - 15)
    }
}
/// Dispatch SQL values after trust checks with independent writers and cycle detection.
fn scalar(o: &Bound<'_, PyAny>, depth: usize, seen: &mut HashSet<usize>) -> PyResult<Vec<u8>> {
    check_depth(depth)?;
    let py = o.py();
    let r = registry(py)?;
    if trusted(o, &r.objstr)? {
        return utf8_sql(&o.str()?.into_any());
    }
    if trusted(o, &r.rawtext)? {
        let v = o.getattr("value")?;
        return utf8_sql(&v);
    }
    if trusted(o, &r.sqlfunction)? {
        let args = o.getattr("args")?;
        let vals = args
            .try_iter()?
            .map(|v| {
                scalar(&v?, depth + 1, seen).and_then(|b| sql_text(py, &b).map(|x| x.unbind()))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let syntax = o.call_method0("syntax")?;
        if vals.is_empty() {
            return utf8_sql(&syntax);
        }
        let t = PyTuple::new(py, vals)?;
        return utf8_sql(&syntax.call_method1("__mod__", (t,))?);
    }
    if trusted(o, &r.sqlinterval)? {
        let arg = scalar(&o.getattr("expr")?, depth + 1, seen)?;
        return utf8_sql(
            &o.call_method0("syntax")?
                .call_method1("__mod__", (sql_text(py, &arg)?,))?,
        );
    }
    drop(r);
    // All classifier candidates share these exact handlers and preceding SQL trust checks.
    match dispatch::classify(o)? {
        dispatch::Kind::Null => return Ok(b"NULL".to_vec()),
        dispatch::Kind::Bool => return Ok(if o.extract::<bool>()? { b"1" } else { b"0" }.to_vec()),
        dispatch::Kind::Int => return Ok(o.str()?.to_str()?.as_bytes().to_vec()),
        dispatch::Kind::Float => return Ok(float_text(o.extract()?).into_bytes()),
        dispatch::Kind::String => return Ok(quote(&utf8_sql(o)?, false)),
        dispatch::Kind::Bytes => {
            let text = o.call_method1("decode", ("ascii", "surrogateescape"))?;
            return Ok(quote(&utf8_sql(&text)?, true));
        }
        dispatch::Kind::Unknown => {}
    }
    if o.is_none() {
        return Ok(b"NULL".to_vec());
    }
    if o.is_instance_of::<PyBool>() {
        return Ok(if o.extract::<bool>()? { b"1" } else { b"0" }.to_vec());
    }
    if o.is_instance_of::<PyInt>() {
        let base = py.import("builtins")?.getattr("int")?;
        let normalized = base.call1((o,))?;
        return Ok(base
            .getattr("__repr__")?
            .call1((normalized,))?
            .cast::<PyString>()?
            .to_str()?
            .as_bytes()
            .to_vec());
    }
    if o.is_instance_of::<PyFloat>() && !is_type(o, "numpy", "generic")? {
        let normalized = py.import("builtins")?.getattr("float")?.call1((o,))?;
        return Ok(float_text(normalized.extract()?).into_bytes());
    }
    if o.is_instance_of::<PyString>() {
        let np_string = py
            .import("sqlaero_rust_kernel")?
            .getattr("_type_cache")?
            .cast::<PyDict>()?
            .get_item("numpy.str_")?;
        let exact_np = np_string.is_some_and(|t| o.get_type().is(&t));
        if o.is_exact_instance_of::<PyString>() || exact_np {
            return Ok(quote(&utf8_sql(o)?, false));
        }
        return Ok(quote(&utf8_sql(&o.str()?.into_any())?, false));
    }
    if o.is_instance_of::<PyBytes>()
        || is_type(o, "builtins", "bytearray")?
        || is_type(o, "builtins", "memoryview")?
    {
        let b = py.import("builtins")?.getattr("bytes")?.call1((o,))?;
        let s = b.call_method1("decode", ("ascii", "surrogateescape"))?;
        return Ok(quote(&utf8_sql(&s)?, true));
    }
    if exact_type(o, "pandas", "NaTType")? {
        return Ok(b"NULL".to_vec());
    }
    if exact_type(o, "decimal", "Decimal")? {
        if !o.call_method0("is_finite")?.extract::<bool>()? {
            return Err(error("nonfinite Decimal"));
        }
        let t = o.str()?;
        let t = t.to_str()?;
        number_syntax(t)?;
        return Ok(t.as_bytes().to_vec());
    }
    if is_type(o, "datetime", "datetime")? {
        return Ok(quote(
            format!("{} {}", date_fields(o)?, time_fields(o)?).as_bytes(),
            false,
        ));
    }
    if is_type(o, "datetime", "date")? {
        return Ok(quote(date_fields(o)?.as_bytes(), false));
    }
    if is_type(o, "datetime", "time")? {
        return Ok(quote(time_fields(o)?.as_bytes(), false));
    }
    if is_type(o, "datetime", "timedelta")? {
        let us = (attr_i(o, "days")? as i128 * 86400 + attr_i(o, "seconds")? as i128) * 1_000_000
            + attr_i(o, "microseconds")? as i128;
        return Ok(duration(us));
    }
    if is_type(o, "time", "struct_time")? {
        let s = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            attr_i(o, "tm_year")?,
            attr_i(o, "tm_mon")?,
            attr_i(o, "tm_mday")?,
            attr_i(o, "tm_hour")?,
            attr_i(o, "tm_min")?,
            attr_i(o, "tm_sec")?
        );
        return Ok(quote(s.as_bytes(), false));
    }
    if is_type(o, "numpy", "generic")? {
        let kind: String = o.getattr("dtype")?.getattr("kind")?.extract()?;
        if kind == "V" && !is_type(o, "numpy", "record")? {
            return Err(PyTypeError::new_err("unsupported NumPy void scalar"));
        }
        if kind == "M" || kind == "m" {
            let v: i64 = o.call_method1("view", ("i8",))?.extract()?;
            return np_time(v, &np_unit(o)?, &kind);
        }
        if kind == "f" {
            let width: usize = o.getattr("dtype")?.getattr("itemsize")?.extract()?;
            let f: f64 = o.extract()?;
            if !f.is_finite() {
                return Ok(b"NULL".to_vec());
            }
            if matches!(width, 2 | 4 | 8) {
                let text = o.str()?;
                let text = text.to_str()?;
                strict_float(text)?;
                return Ok(text.as_bytes().to_vec());
            }
            return Err(PyTypeError::new_err(
                "unsupported NumPy scalar floating width",
            ));
        }
        return scalar(&o.call_method0("item")?, depth + 1, seen);
    }
    if exact_type(o, "pandas", "DataFrame")? {
        let cols = o.getattr("shape")?.get_item(1)?.extract::<usize>()?;
        let rows = o.getattr("shape")?.get_item(0)?.extract::<usize>()?;
        if rows == 0 || cols == 0 {
            return Ok(b"()".to_vec());
        }
        let iloc = o.getattr("iloc")?;
        let mut rs = Vec::with_capacity(rows);
        for i in 0..rows {
            let mut row = Vec::with_capacity(cols);
            for j in 0..cols {
                row.push(scalar(&iloc.get_item((i, j))?, depth + 1, seen)?);
            }
            rs.push(join_row(row, false));
        }
        return Ok(join_row(rs, true));
    }
    if is_type(o, "pandas", "Series")?
        || is_type(o, "pandas", "Index")?
        || is_type(o, "cytimes", "Pddt")?
    {
        return scalar(&o.getattr("values")?, depth + 1, seen);
    }
    let id = o.as_ptr() as usize;
    if !seen.insert(id) {
        return Err(error("cyclic SQL container"));
    }
    let res = (|| {
        if exact_type(o, "numpy", "ndarray")? {
            let ndim: usize = o.getattr("ndim")?.extract()?;
            let rows = ndarray_rows(o, depth + 1, seen)?;
            if rows.is_empty() {
                return Ok(b"()".to_vec());
            }
            return Ok(if ndim == 1 {
                join_row(rows.into_iter().next().unwrap(), false)
            } else {
                join_row(rows.into_iter().map(|r| join_row(r, false)).collect(), true)
            });
        }
        if let Some(items) = container(o)? {
            let nested = !items.is_empty() && container(items[0].bind(py))?.is_some();
            let values = items
                .iter()
                .map(|v| scalar(v.bind(py), depth + 1, seen))
                .collect::<PyResult<_>>()?;
            return Ok(join_row(values, nested));
        }
        Err(PyTypeError::new_err(format!(
            "unsupported SQL type {}",
            o.get_type().name()?
        )))
    })();
    seen.remove(&id);
    res
}
/// Escape supported values to SQL literals or ordered parameter item collections.
fn escape_impl<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    many: bool,
    itemize: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let mut seen = HashSet::new();
    let r = registry(py)?;
    let hook = trusted(data, &r.objstr)?
        || trusted(data, &r.rawtext)?
        || trusted(data, &r.sqlfunction)?
        || trusted(data, &r.sqlinterval)?;
    drop(r);
    if hook || !(many || itemize) {
        return sql_text(py, &scalar(data, 0, &mut seen)?);
    }
    if exact_type(data, "pandas", "DataFrame")? {
        let rows: usize = data.getattr("shape")?.get_item(0)?.extract()?;
        let cols: usize = data.getattr("shape")?.get_item(1)?.extract()?;
        if rows == 0 || cols == 0 {
            return Ok(PyList::empty(py).into_any());
        }
        let iloc = data.getattr("iloc")?;
        let mut out = Vec::new();
        for i in 0..rows {
            let mut row = Vec::new();
            for j in 0..cols {
                row.push(sql_text(py, &scalar(&iloc.get_item((i, j))?, 1, &mut seen)?)?.unbind());
            }
            out.push(PyTuple::new(py, row)?.into_any().unbind());
        }
        return Ok(PyList::new(py, out)?.into_any());
    }
    let values = if is_type(data, "pandas", "Series")?
        || is_type(data, "pandas", "Index")?
        || is_type(data, "cytimes", "Pddt")?
    {
        Some(data.getattr("values")?)
    } else {
        None
    };
    let o = values.as_ref().unwrap_or(data);
    if exact_type(o, "numpy", "ndarray")? {
        let ndim: usize = o.getattr("ndim")?.extract()?;
        let rows = ndarray_rows(o, 0, &mut seen)?;
        let mut out = Vec::new();
        for row in rows {
            let vals = row
                .into_iter()
                .map(|b| sql_text(py, &b).map(|x| x.unbind()))
                .collect::<PyResult<Vec<_>>>()?;
            if ndim == 1 {
                return Ok(if many {
                    PyList::new(py, vals)?.into_any()
                } else {
                    PyTuple::new(py, vals)?.into_any()
                });
            }
            out.push(PyTuple::new(py, vals)?.into_any().unbind());
        }
        return Ok(PyList::new(py, out)?.into_any());
    }
    if let Some(vals) = container(o)? {
        seen.insert(o.as_ptr() as usize);
        let mut out = Vec::new();
        for v in vals {
            let v = v.bind(py);
            if many {
                if let Some(row) = container(v)? {
                    let row = row
                        .into_iter()
                        .map(|x| {
                            scalar(x.bind(py), 1, &mut seen)
                                .and_then(|b| sql_text(py, &b).map(|x| x.unbind()))
                        })
                        .collect::<PyResult<Vec<_>>>()?;
                    out.push(PyTuple::new(py, row)?.into_any().unbind());
                    continue;
                }
            }
            out.push(sql_text(py, &scalar(v, 1, &mut seen)?)?.unbind());
        }
        return Ok(if many {
            PyList::new(py, out)?.into_any()
        } else {
            PyTuple::new(py, out)?.into_any()
        });
    }
    sql_text(py, &scalar(o, 0, &mut seen)?)
}
/// Write strict JSON string escaping without ASCII-escaping ordinary Unicode.
fn json_string(s: &str, out: &mut String) -> PyResult<()> {
    let length = s
        .chars()
        .try_fold(2usize, |n, c| {
            n.checked_add(match c {
                '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
                c if c < ' ' => 6,
                c => c.len_utf8(),
            })
        })
        .ok_or_else(|| error("JSON string length overflow"))?;
    if out.len().checked_add(length).is_none_or(|n| n > MAX_BYTES) {
        return Err(error("JSON exceeds byte budget"));
    }
    out.try_reserve(length)
        .map_err(|_| error("JSON buffer allocation failed"))?;
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(())
}
/// Encode supported Python JSON trees with bounded depth, own syntax and cycle checks.
fn json_encode(
    o: &Bound<'_, PyAny>,
    out: &mut String,
    depth: usize,
    seen: &mut HashSet<usize>,
) -> PyResult<()> {
    check_depth(depth)?;
    if out.len() > MAX_BYTES {
        return Err(error("JSON exceeds byte budget"));
    }
    let py = o.py();
    if o.is_none() {
        out.push_str("null");
        return Ok(());
    }
    if o.is_instance_of::<PyBool>() {
        out.push_str(if o.extract::<bool>()? {
            "true"
        } else {
            "false"
        });
        return Ok(());
    }
    if let Ok(s) = o.cast::<PyString>() {
        json_string(s.to_str()?, out)?;
        return Ok(());
    }
    if o.is_instance_of::<PyInt>() {
        let s = py.get_type::<PyInt>().getattr("__repr__")?.call1((o,))?;
        let s = s.cast::<PyString>()?.to_str()?;
        if s.parse::<i64>().is_err() && s.parse::<u64>().is_err() {
            return Err(error("JSON integer exceeds signed/unsigned 64-bit domain"));
        }
        out.push_str(s);
        return Ok(());
    }
    if o.is_exact_instance_of::<PyFloat>() {
        let f: f64 = o.extract()?;
        if f.is_finite() {
            out.push_str(&float_text(f));
        } else {
            out.push_str("null");
        }
        return Ok(());
    }
    if exact_type(o, "datetime", "time")? && !o.getattr("tzinfo")?.is_none() {
        return Err(PyTypeError::new_err("JSON aware time unsupported"));
    }
    if exact_type(o, "datetime", "datetime")?
        || exact_type(o, "datetime", "date")?
        || exact_type(o, "datetime", "time")?
    {
        let s = o.call_method0("isoformat")?;
        json_string(s.cast::<PyString>()?.to_str()?, out)?;
        return Ok(());
    }
    if exact_type(o, "uuid", "UUID")? {
        json_string(o.str()?.to_str()?, out)?;
        return Ok(());
    }
    if is_type(o, "enum", "Enum")? {
        return json_encode(&o.getattr("value")?, out, depth + 1, seen);
    }
    if is_type(o, "numpy", "generic")? {
        let kind: String = o.getattr("dtype")?.getattr("kind")?.extract()?;
        if kind == "M" {
            let v: i64 = o.call_method1("view", ("i8",))?.extract()?;
            if v == i64::MIN {
                return Err(error("JSON datetime64 NaT unsupported"));
            }
            let b = np_time(v, &json_np_unit(o)?, "M")?;
            let s = std::str::from_utf8(&b[1..b.len() - 1])
                .map_err(|_| error("datetime conversion"))?
                .replace(' ', "T");
            json_string(&s, out)?;
            return Ok(());
        }
        if kind == "f" {
            let width: usize = o.getattr("dtype")?.getattr("itemsize")?.extract()?;
            let f: f64 = o.extract()?;
            if !f.is_finite() {
                out.push_str("null");
            } else if width <= 4 {
                out.push_str(&float32_text(f as f32));
            } else {
                out.push_str(&float_text(f));
            }
            return Ok(());
        }
        if !matches!(kind.as_str(), "b" | "i" | "u") {
            return Err(PyTypeError::new_err("unsupported JSON NumPy scalar"));
        }
        return json_encode(&o.call_method0("item")?, out, depth + 1, seen);
    }
    let id = o.as_ptr() as usize;
    if !seen.insert(id) {
        return Err(error("cyclic JSON object"));
    }
    let res = (|| {
        if let Ok(d) = o.cast::<PyDict>() {
            let pairs: Vec<_> = d.iter().map(|(k, v)| (k.unbind(), v.unbind())).collect();
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let k = k
                    .bind(py)
                    .cast::<PyString>()
                    .map_err(|_| PyTypeError::new_err("JSON object keys must be strings"))?;
                json_string(k.to_str()?, out)?;
                out.push(':');
                json_encode(v.bind(py), out, depth + 1, seen)?;
            }
            out.push('}');
            return Ok(());
        }
        if o.is_instance_of::<PyList>() || o.is_exact_instance_of::<PyTuple>() {
            let vals: Vec<Py<PyAny>> = if let Ok(list) = o.cast::<PyList>() {
                list.iter().map(|v| v.unbind()).collect()
            } else {
                o.cast::<PyTuple>()?.iter().map(|v| v.unbind()).collect()
            };
            out.push('[');
            for (i, v) in vals.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_encode(v.bind(py), out, depth + 1, seen)?;
            }
            out.push(']');
            return Ok(());
        }
        if exact_type(o, "numpy", "ndarray")? {
            return json_array(o, out, depth + 1, seen);
        }
        let dc = py.import("dataclasses")?;
        if dc.getattr("is_dataclass")?.call1((o,))?.extract::<bool>()?
            && !o.is_instance_of::<PyType>()
        {
            let fields = dc.getattr("fields")?.call1((o,))?;
            out.push('{');
            let mut first = true;
            for f in fields.try_iter()? {
                let f = f?;
                let name: String = f.getattr("name")?.extract()?;
                if name.starts_with('_') {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                json_string(&name, out)?;
                out.push(':');
                json_encode(&o.getattr(name.as_str())?, out, depth + 1, seen)?;
            }
            out.push('}');
            return Ok(());
        }
        Err(PyTypeError::new_err(format!(
            "unsupported JSON type {}",
            o.get_type().name()?
        )))
    })();
    seen.remove(&id);
    res
}
/// Append a JSON token only after checking the complete writer growth.
fn json_token(out: &mut String, token: &str) -> PyResult<()> {
    if out
        .len()
        .checked_add(token.len())
        .is_none_or(|n| n > MAX_BYTES)
    {
        return Err(error("JSON exceeds byte budget"));
    }
    out.try_reserve(token.len())
        .map_err(|_| error("JSON buffer allocation failed"))?;
    out.push_str(token);
    Ok(())
}
/// Stream owned dtype bytes in logical array shape without allocating per-cell SQL rows.
fn json_native_shape(
    shape: &[usize],
    bytes: &[u8],
    width: usize,
    kind: &str,
    unit: &str,
    offset: &mut usize,
    out: &mut String,
    depth: usize,
) -> PyResult<()> {
    check_depth(depth)?;
    json_token(out, "[")?;
    for index in 0..shape[0] {
        if index > 0 {
            json_token(out, ",")?;
        }
        if shape.len() > 1 {
            json_native_shape(
                &shape[1..],
                bytes,
                width,
                kind,
                unit,
                offset,
                out,
                depth + 1,
            )?;
            continue;
        }
        let end = offset
            .checked_add(width)
            .ok_or_else(|| error("JSON array offset overflow"))?;
        let b = bytes
            .get(*offset..end)
            .ok_or_else(|| error("JSON array byte length mismatch"))?;
        *offset = end;
        let mut raw = [0u8; 8];
        if cfg!(target_endian = "little") {
            raw[..width].copy_from_slice(b);
        } else {
            for (j, v) in b.iter().rev().enumerate() {
                raw[j] = *v;
            }
        }
        let u = u64::from_le_bytes(raw);
        match kind {
            "b" => json_token(out, if u == 0 { "false" } else { "true" })?,
            "u" => json_token(out, &u.to_string())?,
            "i" => {
                let shift = 64 - width * 8;
                json_token(out, &((u << shift) as i64 >> shift).to_string())?;
            }
            "f" => {
                let text = match width {
                    8 => float_text(f64::from_bits(u)),
                    4 => float32_text(f32::from_bits(u as u32)),
                    2 => float32_text(half_float(u as u16)),
                    _ => return Err(error("unsupported JSON floating width")),
                };
                json_token(out, if text == "NULL" { "null" } else { &text })?;
            }
            "M" => {
                let b = np_time(u as i64, unit, "M")?;
                if b == b"NULL" {
                    return Err(PyTypeError::new_err("JSON datetime64 NaT unsupported"));
                }
                let s = std::str::from_utf8(&b[1..b.len() - 1])
                    .map_err(|_| error("invalid JSON datetime conversion"))?
                    .replace(' ', "T");
                json_string(&s, out)?;
            }
            _ => return Err(PyTypeError::new_err("unsupported JSON dtype kind")),
        }
    }
    json_token(out, "]")?;
    Ok(())
}
/// Encode reviewed native NumPy arrays with a bounded owned snapshot and streaming dtype writer.
fn json_array(
    o: &Bound<'_, PyAny>,
    out: &mut String,
    depth: usize,
    _seen: &mut HashSet<usize>,
) -> PyResult<()> {
    check_depth(depth)?;
    let ndim: usize = o.getattr("ndim")?.extract()?;
    if ndim == 0 || ndim > 32 {
        return Err(error("JSON ndarray dimensions unsupported"));
    }
    let shape: Vec<usize> = o.getattr("shape")?.extract()?;
    if shape.len() != ndim {
        return Err(error("JSON ndarray shape rank mismatch"));
    }
    let count = shape
        .iter()
        .try_fold(1usize, |n, d| n.checked_mul(*d))
        .ok_or_else(|| error("JSON array shape overflow"))?;
    let dtype = o.getattr("dtype")?;
    let kind: String = dtype.getattr("kind")?.extract()?;
    let width: usize = dtype.getattr("itemsize")?.extract()?;
    let contiguous: bool = o.getattr("flags")?.getattr("c_contiguous")?.extract()?;
    let native: bool = dtype.getattr("isnative")?.extract()?;
    if !contiguous || !native {
        return Err(PyTypeError::new_err(
            "JSON NumPy arrays require C-contiguous native-endian layout",
        ));
    }
    if !matches!(kind.as_str(), "b" | "i" | "u" | "f" | "M")
        || !matches!(width, 1 | 2 | 4 | 8)
        || (kind == "b" && width != 1)
        || (kind == "M" && width != 8)
        || (kind == "f" && !matches!(width, 2 | 4 | 8))
    {
        return Err(PyTypeError::new_err("unsupported JSON ndarray dtype"));
    }
    let len = count
        .checked_mul(width)
        .ok_or_else(|| error("JSON snapshot size overflow"))?;
    if len > MAX_BYTES {
        return Err(error("JSON snapshot exceeds byte budget"));
    }
    let unit = if kind == "M" {
        json_np_unit(o)?
    } else {
        String::new()
    };
    let raw = o.call_method1("tobytes", ("C",))?;
    let bytes = raw.cast::<PyBytes>()?.as_bytes();
    if bytes.len() != len {
        return Err(error("JSON snapshot length mismatch"));
    }
    let mut offset = 0;
    json_native_shape(&shape, bytes, width, &kind, &unit, &mut offset, out, depth)?;
    if offset != len {
        return Err(error("JSON snapshot was not fully consumed"));
    }
    Ok(())
}
/// Serialize JSON independently, including supported NumPy and standard rich values.
#[pyfunction]
fn dumps<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    let mut out = String::new();
    json_encode(value, &mut out, 0, &mut HashSet::new()).map_err(|e| {
        let mapped = PyTypeError::new_err(e.to_string());
        mapped.set_cause(py, Some(e));
        mapped
    })?;
    if out.len() > MAX_BYTES {
        return Err(PyTypeError::new_err("JSON exceeds byte budget"));
    }
    Ok(PyBytes::new(py, out.as_bytes()))
}
struct Parser<'a, 'py> {
    data: &'a [u8],
    pos: usize,
    py: Python<'py>,
}
impl<'a, 'py> Parser<'a, 'py> {
    /// Report the exact byte position of a parser grammar failure.
    fn bad(&self) -> PyErr {
        error(format!("invalid JSON at byte {}", self.pos))
    }
    /// Consume only the four whitespace bytes allowed by JSON grammar.
    fn ws(&mut self) {
        while self
            .data
            .get(self.pos)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.pos += 1;
        }
    }
    /// Consume one expected byte using a checked input access.
    fn take(&mut self, b: u8) -> bool {
        if self.data.get(self.pos) == Some(&b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    /// Consume a fixed JSON keyword only when its complete byte spelling matches.
    fn word(&mut self, s: &[u8]) -> bool {
        if self.data.get(self.pos..self.pos + s.len()) == Some(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    /// Read exactly four hexadecimal escape digits with checked bounds.
    fn hex(&mut self) -> PyResult<u32> {
        let mut n = 0;
        for _ in 0..4 {
            let b = *self.data.get(self.pos).ok_or_else(|| self.bad())?;
            self.pos += 1;
            n = n * 16
                + match b {
                    b'0'..=b'9' => (b - b'0') as u32,
                    b'a'..=b'f' => (b - b'a' + 10) as u32,
                    b'A'..=b'F' => (b - b'A' + 10) as u32,
                    _ => return Err(self.bad()),
                };
        }
        Ok(n)
    }
    /// Parse strict UTF-8 JSON strings and assemble valid escaped surrogate pairs.
    fn string(&mut self) -> PyResult<String> {
        if !self.take(b'"') {
            return Err(self.bad());
        }
        let mut out = String::new();
        let mut start = self.pos;
        loop {
            let b = *self.data.get(self.pos).ok_or_else(|| self.bad())?;
            if b == b'"' || b == b'\\' {
                let s = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| self.bad())?;
                out.push_str(s);
                self.pos += 1;
                if b == b'"' {
                    return Ok(out);
                }
                let esc = *self.data.get(self.pos).ok_or_else(|| self.bad())?;
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let mut n = self.hex()?;
                        if (0xd800..=0xdbff).contains(&n) {
                            if !self.word(b"\\u") {
                                return Err(self.bad());
                            }
                            let low = self.hex()?;
                            if !(0xdc00..=0xdfff).contains(&low) {
                                return Err(self.bad());
                            }
                            n = 0x10000 + ((n - 0xd800) << 10) + (low - 0xdc00);
                        }
                        if (0xdc00..=0xdfff).contains(&n) {
                            return Err(self.bad());
                        }
                        out.push(char::from_u32(n).ok_or_else(|| self.bad())?);
                    }
                    _ => return Err(self.bad()),
                }
                start = self.pos;
            } else {
                if b < 32 {
                    return Err(self.bad());
                }
                self.pos += 1;
            }
        }
    }
    /// Parse the complete JSON number grammar and reject nonfinite conversion results.
    fn number(&mut self) -> PyResult<Bound<'py, PyAny>> {
        let start = self.pos;
        self.take(b'-');
        if self.take(b'0') {
        } else {
            if !self
                .data
                .get(self.pos)
                .is_some_and(|b| (b'1'..=b'9').contains(b))
            {
                return Err(self.bad());
            }
            while self.data.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
        }
        let mut fl = false;
        if self.take(b'.') {
            fl = true;
            let at = self.pos;
            while self.data.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if at == self.pos {
                return Err(self.bad());
            }
        }
        if self.take(b'e') || self.take(b'E') {
            fl = true;
            let _ = self.take(b'+') || self.take(b'-');
            let at = self.pos;
            while self.data.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if at == self.pos {
                return Err(self.bad());
            }
        }
        let s = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| self.bad())?;
        if !fl {
            if let Ok(n) = s.parse::<i64>() {
                return n
                    .into_pyobject(self.py)
                    .map(|x| x.into_any())
                    .map_err(Into::into);
            }
            if let Ok(n) = s.parse::<u64>() {
                return n
                    .into_pyobject(self.py)
                    .map(|x| x.into_any())
                    .map_err(Into::into);
            }
        }
        let f = s.parse::<f64>().map_err(|_| self.bad())?;
        if !f.is_finite() {
            return Err(self.bad());
        }
        Ok(PyFloat::new(self.py, f).into_any())
    }
    /// Parse one JSON tree node with independent grammar and bounded recursion.
    fn value(&mut self, depth: usize) -> PyResult<Bound<'py, PyAny>> {
        check_depth(depth)?;
        self.ws();
        match self.data.get(self.pos) {
            Some(b'"') => Ok(PyString::new(self.py, &self.string()?).into_any()),
            Some(b'[') => {
                self.pos += 1;
                let out = PyList::empty(self.py);
                self.ws();
                if self.take(b']') {
                    return Ok(out.into_any());
                }
                loop {
                    out.append(self.value(depth + 1)?)?;
                    self.ws();
                    if self.take(b']') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err(self.bad());
                    }
                }
                Ok(out.into_any())
            }
            Some(b'{') => {
                self.pos += 1;
                let out = PyDict::new(self.py);
                self.ws();
                if self.take(b'}') {
                    return Ok(out.into_any());
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    if !self.take(b':') {
                        return Err(self.bad());
                    }
                    out.set_item(key, self.value(depth + 1)?)?;
                    self.ws();
                    if self.take(b'}') {
                        break;
                    }
                    if !self.take(b',') {
                        return Err(self.bad());
                    }
                }
                Ok(out.into_any())
            }
            Some(b'n') if self.word(b"null") => Ok(self.py.None().into_bound(self.py)),
            Some(b't') if self.word(b"true") => {
                Ok(PyBool::new(self.py, true).to_owned().into_any())
            }
            Some(b'f') if self.word(b"false") => {
                Ok(PyBool::new(self.py, false).to_owned().into_any())
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.bad()),
        }
    }
}
/// Parse complete strict UTF-8 JSON with independent grammar and bounded recursion.
#[pyfunction]
fn loads<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let owned;
    let data = if let Ok(b) = value.cast::<PyBytes>() {
        b.as_bytes()
    } else if let Ok(s) = value.cast::<PyString>() {
        owned = s.to_str()?.as_bytes().to_vec();
        &owned
    } else {
        return Err(PyTypeError::new_err("JSON input must be bytes or str"));
    };
    if data.len() > MAX_BYTES {
        return Err(error("JSON exceeds byte budget"));
    }
    std::str::from_utf8(data).map_err(|_| error("JSON input is not UTF-8"))?;
    let mut p = Parser { data, pos: 0, py };
    let result = p.value(0)?;
    p.ws();
    if p.pos != data.len() {
        return Err(p.bad());
    }
    Ok(result)
}
/// Borrow a validated UTF-8 field token before stricter numeric or calendar grammar checks.
fn ascii<'a>(b: &'a [u8]) -> PyResult<&'a str> {
    std::str::from_utf8(b).map_err(|_| error("numeric/date input must be ASCII"))
}
/// Validate integer syntax and the signed or unsigned 64-bit decode domain.
fn strict_int(s: &str) -> PyResult<()> {
    let body = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error("invalid integer text"));
    }
    if s.parse::<i64>().is_err() && s.parse::<u64>().is_err() {
        return Err(error(
            "integer decode exceeds signed/unsigned 64-bit domain",
        ));
    }
    Ok(())
}
/// Validate full decimal and exponent syntax independently of numeric precision or range.
fn number_syntax(s: &str) -> PyResult<()> {
    if s.trim() != s || s.is_empty() {
        return Err(error("invalid floating text"));
    }
    let body = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    let mut digits = 0;
    let mut exp = false;
    let mut point = false;
    let bs = body.as_bytes();
    let mut i = 0;
    while i < bs.len() {
        match bs[i] {
            b'0'..=b'9' => digits += 1,
            b'.' if !point && !exp => point = true,
            b'e' | b'E' if !exp && digits > 0 => {
                exp = true;
                digits = 0;
                if bs.get(i + 1).is_some_and(|b| matches!(b, b'+' | b'-')) {
                    i += 1;
                }
            }
            _ => return Err(error("invalid floating text")),
        }
        i += 1;
    }
    if digits == 0 {
        return Err(error("invalid floating text"));
    }
    Ok(())
}
/// Parse a validated floating token with correctly rounded Rust conversion and finite checks.
fn strict_float(s: &str) -> PyResult<f64> {
    number_syntax(s)?;
    let f = s
        .parse::<f64>()
        .map_err(|_| error("invalid floating text"))?;
    if !f.is_finite() {
        return Err(error("nonfinite floating decode"));
    }
    Ok(f)
}
/// Validate one to six fractional digits and scale them to integer microseconds.
fn fraction(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
        None
    } else {
        Some(s.parse::<u32>().ok()? * 10u32.pow(6 - s.len() as u32))
    }
}
/// Parse flexible-width date/time fields with exact separators and original-text fallback.
fn date_decode<'py>(py: Python<'py>, s: &str, dt: bool) -> PyResult<Bound<'py, PyAny>> {
    let fallback = || Ok(PyString::new(py, s).into_any());
    let mut split = s.split([' ', 'T']);
    let date = split.next().unwrap_or("");
    let time = split.next();
    if split.next().is_some() || dt != time.is_some() {
        return fallback();
    }
    let fields: Vec<_> = date.split('-').collect();
    if fields.len() != 3 {
        return fallback();
    }
    /// Parse the complete JSON number grammar and reject nonfinite conversion results.
    /// Validate a flexible-width calendar component without accepting arbitrary separators.
    fn number(s: &str, max: usize) -> Option<i32> {
        if s.is_empty() || s.len() > max || !s.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            s.parse().ok()
        }
    }
    let (y, m, d) = match (
        number(fields[0], 4),
        number(fields[1], 2),
        number(fields[2], 2),
    ) {
        (Some(y), Some(m), Some(d)) => (y, m, d),
        _ => return fallback(),
    };
    let module = py.import("datetime")?;
    if !dt {
        return match module.getattr("date")?.call1((y, m, d)) {
            Ok(v) => Ok(v),
            Err(_) => fallback(),
        };
    }
    let time = time.unwrap();
    let (t, us) = if let Some((t, f)) = time.split_once('.') {
        let us = match fraction(f) {
            Some(v) => v,
            None => return fallback(),
        };
        (t, us)
    } else {
        (time, 0)
    };
    let fields: Vec<_> = t.split(':').collect();
    if fields.len() != 3 {
        return fallback();
    }
    let (h, mi, sec) = match (
        number(fields[0], 2),
        number(fields[1], 2),
        number(fields[2], 2),
    ) {
        (Some(h), Some(mi), Some(sec)) => (h, mi, sec),
        _ => return fallback(),
    };
    match module.getattr("datetime")?.call1((y, m, d, h, mi, sec, us)) {
        Ok(v) => Ok(v),
        Err(_) => fallback(),
    }
}
/// Parse signed multi-day time values with checked arithmetic and timedelta range validation.
fn time_decode<'py>(py: Python<'py>, s: &str) -> PyResult<Bound<'py, PyAny>> {
    let fallback = || Ok(PyString::new(py, s).into_any());
    let (neg, s0) = if let Some(v) = s.strip_prefix('-') {
        (true, v)
    } else {
        (false, s)
    };
    let mut parts = s0.split(':');
    let h = parts.next().unwrap_or("");
    let m = parts.next().unwrap_or("");
    let sf = parts.next().unwrap_or("");
    if parts.next().is_some()
        || h.is_empty()
        || !h.bytes().all(|b| b.is_ascii_digit())
        || m.len() != 2
        || !m.bytes().all(|b| b.is_ascii_digit())
    {
        return fallback();
    }
    let (sec, us) = if let Some((sec, f)) = sf.split_once('.') {
        let us = match fraction(f) {
            Some(v) => v,
            None => return fallback(),
        };
        (sec, us)
    } else {
        (sf, 0)
    };
    if sec.len() != 2 || !sec.bytes().all(|b| b.is_ascii_digit()) {
        return fallback();
    }
    let (h, m, sec) = match (h.parse::<i128>(), m.parse::<i128>(), sec.parse::<i128>()) {
        (Ok(h), Ok(m), Ok(sec)) if m < 60 && sec < 60 => (h, m, sec),
        _ => return fallback(),
    };
    let us = match h
        .checked_mul(3600)
        .and_then(|v| v.checked_add(m * 60 + sec))
        .and_then(|v| v.checked_mul(1_000_000))
        .and_then(|v| v.checked_add(us as i128))
    {
        Some(v) => {
            if neg {
                -v
            } else {
                v
            }
        }
        None => return fallback(),
    };
    let days = us.div_euclid(86_400_000_000);
    if !(-999_999_999..=999_999_999).contains(&days) {
        return fallback();
    }
    let rem = us.rem_euclid(86_400_000_000);
    py.import("datetime")?.getattr("timedelta")?.call1((
        days as i64,
        (rem / 1_000_000) as i64,
        (rem % 1_000_000) as i64,
    ))
}
/// Decode a MySQL text-protocol field with strict numeric parsing and date fallback.
fn decode_impl<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyBytes>,
    field_type: u32,
    encoding: &Bound<'py, PyBytes>,
    is_binary: bool,
    use_decimal: bool,
    decode_bit: bool,
    decode_json: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let b = value.as_bytes();
    let enc = ascii(encoding.as_bytes())?;
    match field_type {
        1 | 2 | 3 | 8 | 9 | 13 => {
            let s = ascii(b)?;
            strict_int(s)?;
            py.import("builtins")?.getattr("int")?.call1((s,))
        }
        0 | 246 => {
            let s = ascii(b)?;
            number_syntax(s)?;
            if use_decimal {
                py.import("decimal")?.getattr("Decimal")?.call1((s,))
            } else {
                Ok(PyFloat::new(py, strict_float(s)?).into_any())
            }
        }
        4 | 5 => Ok(PyFloat::new(py, strict_float(ascii(b)?)?).into_any()),
        7 | 12 => date_decode(py, ascii(b)?, true),
        10 | 14 => date_decode(py, ascii(b)?, false),
        11 => time_decode(py, ascii(b)?),
        16 => {
            if !decode_bit {
                return Ok(value.clone().into_any());
            }
            if b.len() > 8 {
                return Err(error("BIT decode exceeds 64 bits"));
            }
            let n = b.iter().fold(0u64, |v, b| (v << 8) | *b as u64);
            Ok(n.into_pyobject(py)?.into_any())
        }
        245 => {
            let text = value.call_method1("decode", (enc, "surrogateescape"))?;
            if decode_json {
                loads(py, &text)
            } else {
                Ok(text)
            }
        }
        247 | 248 => value.call_method1("decode", (enc, "surrogateescape")),
        _ => {
            if is_binary {
                Ok(value.clone().into_any())
            } else {
                value.call_method1("decode", (enc, "surrogateescape"))
            }
        }
    }
}
/// Retain installed public SQLAero exception identities and the original cause.
fn mapped_error(py: Python<'_>, err: PyErr, escape: bool) -> PyErr {
    if err.is_instance_of::<pyo3::exceptions::PyAssertionError>(py) {
        return err;
    }
    let name = if escape {
        if err.is_instance_of::<PyValueError>(py) {
            "EscapeValueError"
        } else if err.is_instance_of::<PyTypeError>(py) {
            "EscapeTypeError"
        } else {
            "EscapeError"
        }
    } else {
        if err.is_instance_of::<PyTypeError>(py) {
            "DecodeTypeError"
        } else {
            "DecodeValueError"
        }
    };
    match py
        .import("sqlaero.errors")
        .and_then(|m| m.getattr(name))
        .and_then(|t| t.call1((err.to_string(),)))
    {
        Ok(v) => {
            let mapped = PyErr::from_value(v);
            mapped.set_cause(py, Some(err));
            mapped
        }
        Err(_) => err,
    }
}
/// Escape supported values, retaining public SQLAero error identities when available.
#[pyfunction(signature=(data, many=false, itemize=true))]
fn escape<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    many: bool,
    itemize: bool,
) -> PyResult<Bound<'py, PyAny>> {
    escape_impl(py, data, many, itemize).map_err(|e| mapped_error(py, e, true))
}
/// Decode a text-protocol value with checked parsing and public errors with causes.
#[pyfunction(signature=(value, field_type, encoding, is_binary, use_decimal=false, decode_bit=false, decode_json=false))]
fn decode<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyBytes>,
    field_type: u32,
    encoding: &Bound<'py, PyBytes>,
    is_binary: bool,
    use_decimal: bool,
    decode_bit: bool,
    decode_json: bool,
) -> PyResult<Bound<'py, PyAny>> {
    decode_impl(
        py,
        value,
        field_type,
        encoding,
        is_binary,
        use_decimal,
        decode_bit,
        decode_json,
    )
    .map_err(|e| {
        if !matches!(
            field_type,
            0 | 1
                | 2
                | 3
                | 4
                | 5
                | 7
                | 8
                | 9
                | 10
                | 11
                | 12
                | 13
                | 14
                | 15
                | 16
                | 245
                | 246
                | 247
                | 248
                | 249
                | 250
                | 251
                | 252
                | 253
                | 254
                | 255
        ) {
            match py
                .import("sqlaero.errors")
                .and_then(|m| m.getattr("DecodeTypeError"))
                .and_then(|t| {
                    t.call1((format!("Cannot decode unsupported field type {field_type}"),))
                }) {
                Ok(v) => {
                    let mapped = PyErr::from_value(v);
                    mapped.set_cause(py, Some(e));
                    mapped
                }
                Err(_) => e,
            }
        } else {
            mapped_error(py, e, false)
        }
    })
}
/// Python extension initialization: own SQL/JSON algorithms and explicit trust registry.
#[pymodule]
fn sqlaero_rust_kernel(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add(
        "__all__",
        vec![
            "escape",
            "decode",
            "dumps",
            "loads",
            "json",
            "configure_types",
            "_registry",
            "_type_cache",
            "_native_dispatch",
            "_set_dispatch_strategy",
            "_dispatch_strategies",
        ],
    )?;
    m.add(
        "__doc__",
        "Independent experimental Rust SQLAero transcoder; backslash SQL mode required.",
    )?;
    let cache = PyDict::new(m.py());
    for (module, name) in KNOWN_TYPES {
        if let Ok(module_obj) = m.py().import(*module) {
            if let Ok(t) = module_obj.getattr(*name) {
                cache.set_item(format!("{module}.{name}"), t)?;
            } else if *module == "pandas" && *name == "NaTType" {
                cache.set_item("pandas.NaTType", module_obj.getattr("NaT")?.get_type())?;
            }
        }
    }
    let dict = PyDict::new(m.py());
    for (name, method) in [
        ("dict_keys", "keys"),
        ("dict_values", "values"),
        ("dict_items", "items"),
    ] {
        cache.set_item(
            format!("builtins.{name}"),
            dict.call_method0(method)?.get_type(),
        )?;
    }
    m.add("_type_cache", cache)?;
    m.add(
        "_native_dispatch",
        Py::new(m.py(), dispatch::Dispatch::new(m.py()))?,
    )?;
    m.add_function(wrap_pyfunction!(dispatch::_set_dispatch_strategy, m)?)?;
    m.add_function(wrap_pyfunction!(dispatch::_dispatch_strategies, m)?)?;
    m.add(
        "_registry",
        Py::new(
            m.py(),
            Registry {
                objstr: vec![],
                rawtext: vec![],
                sqlfunction: vec![],
                sqlinterval: vec![],
            },
        )?,
    )?;
    m.add_function(wrap_pyfunction!(configure_types, m)?)?;
    m.add_function(wrap_pyfunction!(escape, m)?)?;
    m.add_function(wrap_pyfunction!(decode, m)?)?;
    m.add_function(wrap_pyfunction!(dumps, m)?)?;
    m.add_function(wrap_pyfunction!(loads, m)?)?;
    let json = PyModule::new(m.py(), "sqlaero_rust_kernel.json")?;
    json.add("__doc__", "Independent bounded JSON encoder and parser.")?;
    json.add("dumps", m.getattr("dumps")?)?;
    json.add("loads", m.getattr("loads")?)?;
    m.add_submodule(&json)?;
    m.py()
        .import("sys")?
        .getattr("modules")?
        .set_item("sqlaero_rust_kernel.json", &json)?;
    Ok(())
}
