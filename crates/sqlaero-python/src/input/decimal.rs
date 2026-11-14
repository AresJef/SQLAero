//! Exact Decimal acquisition through a qualified native string protocol.
//!
//! One canonical base snapshot is the explicitly approved atomic CPython
//! source interval. Original control surrounds it; later inspection/rendering
//! is bounded. Targets select their own policy without normalizing the source
//! coefficient, scale, signed zero or nonfinite category in this reader.

use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyString, PyStringData, PyTuple, PyType};
use sqlaero_transcode::decimal::{Digits, Header, Inspect, Render, Special};
use sqlaero_transcode::value::Consumer;
use std::ffi::CStr;

/// Select the immutable native protocol without retaining any new Python
/// objects. Mutable and pure-Python providers retain their original hooks.
pub(super) fn native_type(typ: &Bound<'_, PyType>) -> PyResult<bool> {
    let py = typ.py();
    if !typ.get_type().is(py.get_type::<PyType>()) {
        return Ok(false);
    }
    let flags: std::ffi::c_ulong = errors::attribute(typ.as_any(), c"__flags__")?.extract()?;
    if flags & pyo3::ffi::Py_TPFLAGS_IMMUTABLETYPE == 0 {
        return Ok(false);
    }
    let native_str = errors::attribute(py.get_type::<PyAny>().as_any(), c"__str__")?;
    let native_method = errors::attribute(py.get_type::<PyDict>().as_any(), c"copy")?;
    for (name, reference) in [(c"__str__", native_str), (c"as_tuple", native_method)] {
        let Some(descriptor) = optional_attribute(typ.as_any(), name)? else {
            return Ok(false);
        };
        if !descriptor.get_type().is(reference.get_type()) {
            return Ok(false);
        }
        let Some(owner) = optional_attribute(&descriptor, c"__objclass__")? else {
            return Ok(false);
        };
        if !owner.is(typ) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn optional_attribute<'py>(
    value: &Bound<'py, PyAny>,
    name: &CStr,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    match errors::attribute(value, name) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_instance_of::<PyAttributeError>(value.py()) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Module-owned canonical source API. Only immutable native base slots may be
/// cached; the legacy nonnative source retains its live base as_tuple protocol.
pub(super) struct Api {
    typ: Py<PyType>,
    string: Option<pyo3::ffi::reprfunc>,
}

impl Api {
    pub(super) fn new(typ: &Bound<'_, PyType>) -> PyResult<Self> {
        let string = if native_type(typ)? {
            // SAFETY: the live retained immutable native base owns this public
            // tp_str slot. Its documented signature is reprfunc.
            let slot =
                unsafe { pyo3::ffi::PyType_GetSlot(typ.as_ptr().cast(), pyo3::ffi::Py_tp_str) };
            if slot.is_null() {
                return Err(errors::builtin_message::<PyTypeError>(
                    typ.py(),
                    "canonical Decimal has no public string slot",
                ));
            }
            // SAFETY: the checked public tp_str slot has this exact signature.
            Some(unsafe { std::mem::transmute::<*mut std::ffi::c_void, pyo3::ffi::reprfunc>(slot) })
        } else {
            None
        };
        Ok(Self {
            typ: typ.clone().unbind(),
            string,
        })
    }

    pub(super) fn typ<'py>(&self, py: Python<'py>) -> &Bound<'py, PyType> {
        self.typ.bind(py)
    }

    /// Acquire once, then lend original facts only for immediate consumption.
    /// The approved native source call itself is atomic inside CPython; every
    /// later parse and target fragment returns through the original Control.
    pub(super) fn read(
        &self,
        value: &Bound<'_, PyAny>,
        budget: Option<&dyn Checkpoint>,
        consume: impl FnOnce(Header, Digits<'_>) -> PyResult<()>,
    ) -> PyResult<()> {
        let py = value.py();
        if let Some(string) = self.string {
            let source = super::control::callback(py, budget, || {
                // SAFETY: the source is a qualified actual Decimal subtype,
                // retained across its canonical base slot. This skips every
                // subclass replacement without inspecting private layouts.
                unsafe { Bound::from_owned_ptr_or_err(py, string(value.as_ptr())) }
            })?
            .cast_into::<PyString>()?;
            // SAFETY: this owned canonical str is immutable and attached for
            // all bounded reads and callbacks below. No raw view is detached.
            let PyStringData::Ucs1(bytes) = (unsafe { source.data()? }) else {
                return Err(errors::builtin_message::<PyTypeError>(
                    py,
                    "canonical Decimal snapshot must be ASCII",
                ));
            };
            let (header, digits) = parse(py, bytes, budget)?;
            consume(header, digits)
        } else {
            let parts = super::control::callback(py, budget, || {
                let method = errors::attribute(self.typ(py).as_any(), c"as_tuple")?;
                method.call1(crate::output::tuple_from_ready(py, [value.clone()])?)
            })?;
            let parts = parts.cast::<PyTuple>()?;
            let sign = parts.get_item(0)?.cast::<PyInt>()?.extract::<u8>()? != 0;
            let exponent = parts.get_item(2)?;
            let category = if exponent.is_exact_instance_of::<PyInt>() {
                None
            } else if let Ok(label) = exponent.cast::<PyString>() {
                // The standard as_tuple marker is a single immutable ASCII
                // character; do not invoke a user string conversion.
                // SAFETY: an actual retained str/subclass is required above;
                // these public base-storage APIs do not call __len__/__str__.
                let length = unsafe { pyo3::ffi::PyUnicode_GetLength(label.as_ptr()) };
                if length < 0 {
                    return Err(PyErr::fetch(py));
                }
                if length == 1 {
                    // SAFETY: the checked base string has exactly one point.
                    Some(
                        match u8::try_from(unsafe {
                            pyo3::ffi::PyUnicode_ReadChar(label.as_ptr(), 0)
                        }) {
                            Ok(b'F') => Special::Infinity,
                            Ok(b'N') => Special::SignalingNaN,
                            _ => Special::QuietNaN,
                        },
                    )
                } else {
                    Some(Special::QuietNaN)
                }
            } else {
                Some(Special::QuietNaN)
            };
            let exponent = if category.is_none() {
                exponent.extract::<i64>()?
            } else {
                0
            };
            let source = parts.get_item(1)?;
            let source = source.cast::<PyTuple>()?;
            let mut digits = Vec::new();
            digits
                .try_reserve_exact(source.len())
                .map_err(|_| errors::memory_error(py))?;
            let mut inspected = Inspect::default();
            for index in 0..source.len() {
                if index != 0 && index.is_multiple_of(4096) {
                    super::checkpoint(py, budget)?;
                }
                let digit = source.get_item(index)?.cast::<PyInt>()?.extract::<u8>()?;
                inspected
                    .digit(digit)
                    .map_err(|error| malformed(py, error))?;
                digits.push(digit);
            }
            super::control::bytes(py, budget, source.len().min(4096))?;
            let header = if let Some(category) = category {
                Header::special(sign, category)
            } else {
                inspected
                    .finish(sign, exponent)
                    .map_err(|error| malformed(py, error))?
            };
            consume(header, Digits::Values(&digits))
        }
    }
}

fn malformed(py: Python<'_>, reason: &'static str) -> PyErr {
    errors::builtin_format::<PyValueError>(
        py,
        format_args!("invalid canonical Decimal source: {reason}"),
    )
}

/// Parse source ASCII without an extra full coefficient buffer or SQL policy.
fn parse<'a>(
    py: Python<'_>,
    text: &'a [u8],
    budget: Option<&dyn Checkpoint>,
) -> PyResult<(Header, Digits<'a>)> {
    let negative = text.first() == Some(&b'-');
    let start = usize::from(negative || text.first() == Some(&b'+'));
    let unsigned = &text[start..];
    let special = if unsigned == b"Infinity" {
        Some((Special::Infinity, &b""[..]))
    } else if let Some(payload) = unsigned.strip_prefix(b"sNaN") {
        Some((Special::SignalingNaN, payload))
    } else {
        unsigned
            .strip_prefix(b"NaN")
            .map(|payload| (Special::QuietNaN, payload))
    };
    if let Some((category, payload)) = special {
        for (index, byte) in payload.iter().enumerate() {
            if index != 0 && index.is_multiple_of(4096) {
                super::checkpoint(py, budget)?;
            }
            if !byte.is_ascii_digit() {
                return Err(malformed(py, "invalid nonfinite payload"));
            }
        }
        super::control::bytes(py, budget, text.len().min(4096))?;
        return Ok((
            Header::special(negative, category),
            Digits::Ascii(payload, &[]),
        ));
    }
    let mut at = start;
    let mut point = None;
    let mut fractional = 0usize;
    let mut inspected = Inspect::default();
    while let Some(&byte) = text.get(at) {
        if at != start && at.is_multiple_of(4096) {
            super::checkpoint(py, budget)?;
        }
        if byte == b'E' || byte == b'e' {
            break;
        }
        if byte == b'.' && point.is_none() {
            point = Some(at);
        } else if byte.is_ascii_digit() {
            inspected
                .digit(byte - b'0')
                .map_err(|error| malformed(py, error))?;
            fractional += usize::from(point.is_some());
        } else {
            return Err(malformed(py, "invalid coefficient character"));
        }
        at += 1;
    }
    let end = at;
    let mut exponent = 0i128;
    if at != text.len() {
        at += 1;
        let negative = text.get(at) == Some(&b'-');
        at += usize::from(negative || text.get(at) == Some(&b'+'));
        let digits_start = at;
        while let Some(&byte) = text.get(at) {
            if at.is_multiple_of(4096) {
                super::checkpoint(py, budget)?;
            }
            if !byte.is_ascii_digit() {
                return Err(malformed(py, "invalid exponent character"));
            }
            exponent = exponent
                .checked_mul(10)
                .and_then(|n| n.checked_add(i128::from(byte - b'0')))
                .ok_or_else(|| malformed(py, "source exponent exceeds accurate native range"))?;
            at += 1;
        }
        if at == digits_start {
            return Err(malformed(py, "empty exponent"));
        }
        if negative {
            exponent = -exponent;
        }
    }
    let exponent = exponent
        .checked_sub(fractional as i128)
        .and_then(|n| i64::try_from(n).ok())
        .ok_or_else(|| malformed(py, "source exponent exceeds accurate native range"))?;
    let header = inspected
        .finish(negative, exponent)
        .map_err(|error| malformed(py, error))?;
    let digits = match point {
        None => Digits::Ascii(&text[start..end], &[]),
        Some(point) => Digits::Ascii(&text[start..point], &text[point + 1..end]),
    };
    super::control::bytes(py, budget, text.len().min(4096))?;
    Ok((header, digits))
}

/// Consume the same coefficient with a target-selected layout and native sink.
/// Python control remains here, never inside a pure native target.
pub(super) fn consume<C: Consumer + ?Sized>(
    py: Python<'_>,
    header: Header,
    digits: Digits<'_>,
    target: &mut C,
    context: &C::Context,
    budget: Option<&dyn Checkpoint>,
    failure: impl Fn(C::Error) -> PyErr,
) -> PyResult<()> {
    let Some(layout) = target.decimal_start(header, context).map_err(&failure)? else {
        return Ok(());
    };
    let render = Render::new(digits, layout).map_err(|error| malformed(py, error))?;
    if layout.len() <= 128 {
        fragments::<C, 128>(py, render, target, context, budget, &failure)?;
    } else {
        fragments::<C, 4096>(py, render, target, context, budget, &failure)?;
    }
    target.decimal_end(context).map_err(failure)
}

fn fragments<C: Consumer + ?Sized, const N: usize>(
    py: Python<'_>,
    mut render: Render<'_>,
    target: &mut C,
    context: &C::Context,
    budget: Option<&dyn Checkpoint>,
    failure: &impl Fn(C::Error) -> PyErr,
) -> PyResult<()> {
    let mut chunk = [0; N];
    loop {
        let n = render.read(&mut chunk);
        if n == 0 {
            return Ok(());
        }
        super::control::bytes(py, budget, n)?;
        let text = std::str::from_utf8(&chunk[..n])
            .map_err(|_| malformed(py, "native decimal renderer emitted non-ASCII"))?;
        target.decimal_fragment(text, context).map_err(failure)?;
    }
}

/// Add bounded original component facts only for an actual target range error.
pub(super) fn note(py: Python<'_>, error: &PyErr, header: Header, digits: Digits<'_>) {
    if error.is_instance_of::<errors::DataError>(py) {
        let mut preview = [0; 16];
        if let Ok(n) = digits.preview(header, &mut preview) {
            errors::note_format(
                py,
                error,
                format_args!(
                    "failed Decimal components: sign={}, digits={:?}{} ({} digits), exponent={}",
                    u8::from(header.negative()),
                    &preview[..n],
                    if header.digits() > 16 {
                        " [truncated]"
                    } else {
                        ""
                    },
                    header.digits(),
                    header.exponent(),
                ),
            );
        }
    }
}
