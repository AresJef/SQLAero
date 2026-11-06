//! Immutable TLS diagnostics, converted lazily at the Python boundary.

use crate::{errors, output};
use pyo3::prelude::*;
use pyo3::types::{PyDateTime, PyString, PyTzInfo};
use sqlaero_core::error::ClientErrorCode;
use sqlaero_core::transport::tls::{
    Mode,
    info::{Certificate, ContentError, Details, DetailsError, Snapshot, UtcDateTime},
};
use std::sync::{Arc, Mutex, MutexGuard};

struct Cache<const N: usize>(Mutex<[Option<Py<PyAny>>; N]>);

impl<const N: usize> Default for Cache<N> {
    fn default() -> Self {
        Self(Mutex::new(std::array::from_fn(|_| None)))
    }
}

impl<const N: usize> Cache<N> {
    fn lock(&self, py: Python<'_>) -> PyResult<MutexGuard<'_, [Option<Py<PyAny>>; N]>> {
        match self.0.lock() {
            Ok(cache) => Ok(cache),
            Err(error) => {
                drop(error.into_inner());
                Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "TLS diagnostic cache was invalidated by a native failure",
                ))
            }
        }
    }
    fn value<'py>(
        &self,
        py: Python<'py>,
        index: usize,
        make: impl FnOnce() -> PyResult<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        {
            let cache = self.lock(py)?;
            if let Some(value) = &cache[index] {
                return Ok(value.clone_ref(py).into_bound(py));
            }
        }
        // No Python constructor, signal check, callback, or native preparation
        // runs while a cache lock is held. Failed candidates are never cached.
        let value = make()?;
        // Native parsing can finish while a signal is pending. Observe it
        // before publishing a completed Python diagnostic candidate.
        py.check_signals()?;
        let winner = {
            let mut cache = self.lock(py)?;
            if let Some(winner) = &cache[index] {
                winner.clone_ref(py)
            } else {
                cache[index] = Some(value.clone().unbind());
                value.clone().unbind()
            }
        };
        Ok(winner.into_bound(py))
    }
}

fn text<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    PyString::from_bytes(py, value.as_bytes()).map(Bound::into_any)
}

fn integer<'py>(py: Python<'py>, value: u64) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: Python is attached; this exact scalar constructor returns a new
    // reference or its original allocation failure, and copies its native input.
    unsafe { Bound::from_owned_ptr_or_err(py, pyo3::ffi::PyLong_FromUnsignedLongLong(value)) }
}

fn boolean<'py>(py: Python<'py>, value: bool) -> Bound<'py, PyAny> {
    match value.into_pyobject(py) {
        Ok(value) => value.to_owned().into_any(),
        Err(never) => match never {},
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Disabled => "DISABLED",
        Mode::Required => "REQUIRED",
        Mode::VerifyCa => "VERIFY_CA",
        Mode::VerifyIdentity => "VERIFY_IDENTITY",
    }
}

/// Read-only facts for one actual TLS connection, independent of its lifetime.
/// Accessing or displaying this object never sends SQL or performs network I/O.
#[pyclass(frozen, name = "TLSInfo", module = "sqlaero", weakref)]
pub(crate) struct TlsInfo {
    snapshot: Arc<Snapshot>,
    cache: Cache<11>,
}

impl TlsInfo {
    pub(crate) fn new(snapshot: Arc<Snapshot>) -> Self {
        Self {
            snapshot,
            cache: Cache::default(),
        }
    }

    fn value<'py>(&self, py: Python<'py>, field: usize) -> PyResult<Bound<'py, PyAny>> {
        self.cache.value(py, field, || match field {
            0 => text(py, mode_name(self.snapshot.mode)),
            1 => text(py, self.snapshot.min_version.as_str()),
            2 => self.snapshot.max_version.map_or_else(
                || Ok(py.None().into_bound(py)),
                |version| text(py, version.as_str()),
            ),
            3 => self
                .snapshot
                .server_name
                .as_deref()
                .map_or_else(|| Ok(py.None().into_bound(py)), |name| text(py, name)),
            4 => text(py, self.snapshot.version.as_str()),
            5 => text(py, self.snapshot.cipher),
            6 => integer(py, u64::from(self.snapshot.cipher_id)),
            7 => Ok(boolean(py, self.snapshot.session_reused)),
            8 => {
                let mut items = Vec::new();
                items
                    .try_reserve_exact(self.snapshot.peer_certificates.len())
                    .map_err(|_| errors::memory_error(py))?;
                for (index, certificate) in self.snapshot.peer_certificates.iter().enumerate() {
                    items.push(
                        Py::new(py, TlsCertificate::new(Arc::clone(certificate), index))?
                            .into_any(),
                    );
                    py.check_signals()?;
                }
                output::tuple_from_ready_vec(py, items).map(Bound::into_any)
            }
            _ => Err(errors::client(
                py,
                ClientErrorCode::InternalState,
                "unknown TLSInfo field",
            )),
        })
    }

    fn display<'py>(&self, py: Python<'py>, multiline: bool) -> PyResult<Bound<'py, PyString>> {
        self.cache.value(py, if multiline { 9 } else { 10 }, || {
            let sep = if multiline { ",\n    " } else { ", " };
            let start = if multiline { "TLSInfo(\n    " } else { "TLSInfo(" };
            let end = if multiline { "\n)" } else { ")" };
            let maximum = self.snapshot.max_version.map(|v| v.as_str());
            let maximum = maximum.map_or_else(|| Ok("None".to_owned()), |v| errors::message(py, format_args!("{v:?}")))?;
            let name = self.snapshot.server_name.as_deref().map_or_else(|| Ok("None".to_owned()), |v| errors::message(py, format_args!("{v:?}")))?;
            let rendered = errors::message(py, format_args!("{start}mode={:?}{sep}min_version={:?}{sep}max_version={maximum}{sep}server_name={name}{sep}version={:?}{sep}cipher={:?}{sep}cipher_id={}{sep}session_reused={}{sep}peer_certificates=<{} certificates>{end}",
                mode_name(self.snapshot.mode), self.snapshot.min_version.as_str(), self.snapshot.version.as_str(), self.snapshot.cipher,
                self.snapshot.cipher_id, if self.snapshot.session_reused { "True" } else { "False" }, self.snapshot.peer_certificates.len()))?;
            text(py, &rendered)
        })?.cast_into::<PyString>().map_err(Into::into)
    }
}

#[pymethods]
impl TlsInfo {
    /// The resolved verification policy used by this TLS connection.
    #[getter]
    fn mode<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 0)
    }
    /// The normalized configured minimum TLS version, not the negotiated result.
    #[getter]
    fn min_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 1)
    }
    /// The extra configured maximum TLS version, or None when not restricted.
    #[getter]
    fn max_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 2)
    }
    /// The actual configured DNS/IP target, or None for an unnamed Unix target.
    #[getter]
    fn server_name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 3)
    }
    /// The actual negotiated protocol version, normalized as TLSv1.2 or TLSv1.3.
    #[getter]
    fn version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 4)
    }
    /// The actual negotiated cipher suite's canonical IANA name.
    #[getter]
    fn cipher<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 5)
    }
    /// The exact 16-bit TLS cipher suite number as int.
    #[getter]
    fn cipher_id<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 6)
    }
    /// True only when the native TLS engine reports a resumed handshake.
    #[getter]
    fn session_reused<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 7)
    }
    /// An immutable tuple of read-only peer certificates, in original order.
    /// This is the saved peer identity, not the verifier's constructed trust path.
    #[getter]
    fn peer_certificates<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 8)
    }
    /// Return a multiline str; peer certificates appear only as a count summary.
    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.display(py, true)
    }
    /// Return a compact str without parsing or expanding any peer certificate.
    fn __repr__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.display(py, false)
    }
}

/// One read-only peer certificate, retained independently of its original chain.
/// Structured details and SHA-256 are prepared only when explicitly requested.
#[pyclass(frozen, name = "TLSCertificate", module = "sqlaero", weakref)]
pub(crate) struct TlsCertificate {
    certificate: Arc<Certificate>,
    index: usize,
    cache: Cache<16>,
}

const FIELDS: [&str; 12] = [
    "subject",
    "issuer",
    "serial_number",
    "version",
    "not_before",
    "not_after",
    "dns_names",
    "ip_addresses",
    "is_ca",
    "signature_algorithm_oid",
    "public_key_algorithm_oid",
    "fingerprint_sha256",
];

impl TlsCertificate {
    fn new(certificate: Arc<Certificate>, index: usize) -> Self {
        Self {
            certificate,
            index,
            cache: Cache::default(),
        }
    }
    fn content(&self, py: Python<'_>, field: usize, source: &ContentError) -> PyErr {
        errors::tls_certificate(py, self.index, FIELDS[field], source)
    }
    fn details(&self, py: Python<'_>, field: usize) -> PyResult<&Details> {
        self.prepared_details(py)?
            .map_err(|source| self.content(py, field, source))
    }
    fn prepared_details(&self, py: Python<'_>) -> PyResult<Result<&Details, &ContentError>> {
        match self.certificate.details() {
            Ok(details) => Ok(Ok(details)),
            Err(DetailsError::Content(error)) => Ok(Err(error)),
            Err(DetailsError::Allocation(_)) => Err(errors::memory_error(py)),
            Err(DetailsError::Invariant(message)) => {
                Err(errors::client(py, ClientErrorCode::InternalState, message))
            }
        }
    }
    fn datetime<'py>(
        &self,
        py: Python<'py>,
        field: usize,
        value: UtcDateTime,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !(1..=9999).contains(&value.year) || !value.nanosecond.is_multiple_of(1000) {
            return Err(self.content(
                py,
                field,
                &ContentError::representation(
                    "validity time cannot be represented exactly as Python datetime",
                ),
            ));
        }
        let utc = PyTzInfo::utc(py)?;
        PyDateTime::new(
            py,
            value.year,
            value.month,
            value.day,
            value.hour,
            value.minute,
            value.second,
            value.nanosecond / 1000,
            Some(&utc),
        )
        .map(Bound::into_any)
    }
    fn names<'py>(
        &self,
        py: Python<'py>,
        field: usize,
        names: &Result<Vec<String>, ContentError>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let names = names
            .as_ref()
            .map_err(|source| self.content(py, field, source))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(names.len())
            .map_err(|_| errors::memory_error(py))?;
        for name in names {
            values.push(text(py, name)?.unbind());
            py.check_signals()?;
        }
        output::tuple_from_ready_vec(py, values).map(Bound::into_any)
    }
    fn fingerprint(&self) -> [u8; 64] {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = [0; 64];
        for (index, byte) in self.certificate.fingerprint().iter().enumerate() {
            output[index * 2] = HEX[usize::from(byte >> 4)];
            output[index * 2 + 1] = HEX[usize::from(byte & 15)];
        }
        output
    }
    fn value<'py>(&self, py: Python<'py>, field: usize) -> PyResult<Bound<'py, PyAny>> {
        self.cache.value(py, field, || {
            py.check_signals()?;
            if field == 11 {
                return PyString::from_bytes(py, &self.fingerprint()).map(Bound::into_any);
            }
            if field == 12 {
                let der = self.certificate.der();
                let length = isize::try_from(der.len()).map_err(|_| errors::memory_error(py))?;
                // SAFETY: the native certificate owns this exact readable DER
                // slice throughout the attached constructor, which copies it.
                return unsafe {
                    Bound::from_owned_ptr_or_err(
                        py,
                        pyo3::ffi::PyBytes_FromStringAndSize(der.as_ptr().cast(), length),
                    )
                };
            }
            if field == 13 {
                let pem = self.certificate.pem().map_err(|error| {
                    if error.is_resource() {
                        errors::memory_error(py)
                    } else {
                        errors::client_format(
                            py,
                            ClientErrorCode::InternalState,
                            format_args!("cannot encode original certificate DER: {error}"),
                        )
                    }
                })?;
                return text(py, &pem);
            }
            let details = self.details(py, field)?;
            match field {
                0 => text(
                    py,
                    details
                        .subject
                        .as_ref()
                        .map_err(|source| self.content(py, field, source))?,
                ),
                1 => text(
                    py,
                    details
                        .issuer
                        .as_ref()
                        .map_err(|source| self.content(py, field, source))?,
                ),
                2 => errors::code_object(py, &details.serial_number).map(Bound::into_any),
                3 => integer(py, u64::from(details.version)),
                4 => self.datetime(py, field, details.not_before),
                5 => self.datetime(py, field, details.not_after),
                6 => self.names(py, field, &details.dns_names),
                7 => self.names(py, field, &details.ip_addresses),
                8 => Ok(boolean(
                    py,
                    *details
                        .is_ca
                        .as_ref()
                        .map_err(|source| self.content(py, field, source))?,
                )),
                9 => text(
                    py,
                    details
                        .signature_algorithm_oid
                        .as_ref()
                        .map_err(|source| self.content(py, field, source))?,
                ),
                10 => text(
                    py,
                    details
                        .public_key_algorithm_oid
                        .as_ref()
                        .map_err(|source| self.content(py, field, source))?,
                ),
                _ => Err(errors::client(
                    py,
                    ClientErrorCode::InternalState,
                    "unknown TLSCertificate field",
                )),
            }
        })
    }

    fn field_error(&self, py: Python<'_>, field: usize) -> PyResult<Option<&ContentError>> {
        if field == 11 {
            return Ok(None);
        }
        let details = match self.prepared_details(py)? {
            Ok(details) => details,
            Err(error) => return Ok(Some(error)),
        };
        Ok(match field {
            0 => details.subject.as_ref().err(),
            1 => details.issuer.as_ref().err(),
            6 => details.dns_names.as_ref().err(),
            7 => details.ip_addresses.as_ref().err(),
            8 => details.is_ca.as_ref().err(),
            9 => details.signature_algorithm_oid.as_ref().err(),
            10 => details.public_key_algorithm_oid.as_ref().err(),
            _ => None,
        })
    }

    fn display<'py>(&self, py: Python<'py>, multiline: bool) -> PyResult<Bound<'py, PyString>> {
        self.cache.value(py, if multiline { 14 } else { 15 }, || {
            let mut rendered = String::new();
            let mut append = |value: &str| -> PyResult<()> {
                rendered.try_reserve(value.len()).map_err(|_| errors::memory_error(py))?;
                rendered.push_str(value);
                Ok(())
            };
            append(if multiline { "TLSCertificate(\n    " } else { "TLSCertificate(" })?;
            for (field, name) in FIELDS.iter().enumerate() {
                if field != 0 { append(if multiline { ",\n    " } else { ", " })?; }
                append(name)?;
                append("=")?;
                if let Some(error) = self.field_error(py, field)? {
                    append(&errors::message(py, format_args!("<unavailable: {error}>"))?)?;
                    continue;
                }
                if matches!(field, 6 | 7) {
                    let details = self.details(py, field)?;
                    let names = if field == 6 { &details.dns_names } else { &details.ip_addresses };
                    let names = names.as_ref().map_err(|source| self.content(py, field, source))?;
                    if names.len() > 1 {
                        // Summarize the whole collection, rather than truncating
                        // an arbitrary number of entries or materializing a
                        // Python tuple merely to display it. Empty and single
                        // values retain the documented complete tuple form.
                        py.check_signals()?;
                        append(&errors::message(py, format_args!(
                            "<{} entries; complete tuple available as .{name}>", names.len()
                        ))?)?;
                        continue;
                    }
                }
                if field == 2 {
                    append(&errors::message(py, format_args!("{}", self.details(py, field)?.serial_number))?)?;
                    continue;
                }
                if matches!(field, 4 | 5) {
                    let details = self.details(py, field)?;
                    let value = if field == 4 { details.not_before } else { details.not_after };
                    if !(1..=9999).contains(&value.year) || !value.nanosecond.is_multiple_of(1000) {
                        append("<unavailable: validity time cannot be represented exactly as Python datetime>")?;
                        continue;
                    }
                }
                // Format one final value at a time; no full property tree is
                // staged before rendering. Resource/signal failures propagate.
                let value = self.value(py, field)?;
                let representation = value.repr()?;
                append(representation.to_str()?)?;
            }
            append(if multiline { "\n)" } else { ")" })?;
            text(py, &rendered)
        })?.cast_into::<PyString>().map_err(Into::into)
    }
}

#[pymethods]
impl TlsCertificate {
    /// The complete Subject DN; decoded attribute values are quoted and escaped.
    #[getter]
    fn subject<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 0)
    }
    /// The complete Issuer DN, with the same display convention as subject.
    #[getter]
    fn issuer<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 1)
    }
    /// The exact certificate serial number as an arbitrary-precision int.
    #[getter]
    fn serial_number<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 2)
    }
    /// The human X.509 version number; v3 is 3, independent of the TLS version.
    #[getter]
    fn version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 3)
    }
    /// The absolute validity start as datetime.datetime with timezone.utc.
    #[getter]
    fn not_before<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 4)
    }
    /// The absolute validity end as datetime.datetime with timezone.utc.
    #[getter]
    fn not_after<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 5)
    }
    /// SAN DNS names as tuple[str, ...], preserving order and duplicates.
    /// Missing DNS names produce (); malformed dependent content raises DataError.
    #[getter]
    fn dns_names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 6)
    }
    /// SAN IPv4/IPv6 addresses as canonical strings, preserving order/duplicates.
    #[getter]
    fn ip_addresses<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 7)
    }
    /// The basicConstraints CA bit; missing means False, malformed raises DataError.
    #[getter]
    fn is_ca<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 8)
    }
    /// The outer certificate signature algorithm's dotted-decimal OID.
    #[getter]
    fn signature_algorithm_oid<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 9)
    }
    /// The SubjectPublicKeyInfo public-key algorithm's dotted-decimal OID.
    #[getter]
    fn public_key_algorithm_oid<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 10)
    }
    /// SHA-256 of original DER as 64 lowercase hex characters, computed once.
    /// This property remains available when structured certificate details fail.
    #[getter]
    fn fingerprint_sha256<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 11)
    }
    /// Return exact original DER as bytes, independent of detail parsing.
    fn to_der<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 12)
    }
    /// Return the same DER as CERTIFICATE PEM str, LF/64-column/final newline.
    fn to_pem<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.value(py, 13)
    }
    /// Return all twelve fields as a multiline str; explicit use may parse details.
    /// Multi-entry DNS/IP collections show their total count and complete-property
    /// name. Empty and single-entry tuples retain their complete representation.
    /// Deterministic content failures appear as unavailable markers.
    fn __str__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.display(py, true)
    }
    /// Return a compact str in the same field order and DNS/IP summary policy.
    /// No PEM or complete multi-entry name tuple is expanded by display.
    fn __repr__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyString>> {
        self.display(py, false)
    }
}
