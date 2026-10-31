//! Strict boundary configuration, shared path parsing and reusable TLS material.

use num_bigint::BigUint;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyInt, PyString};
use rustls::sign::CertifiedKey;
use rustls_pki_types::{CertificateDer, ServerName};
use sqlaero_core::{
    config::size,
    transport::{
        deadline::{IoBudget, TimeoutKind},
        tls::{self, MinimumVersion, Mode},
    },
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// A real presence marker: explicit Python None is never mistaken for omission.
#[derive(Default)]
pub(crate) struct Argument<'py>(pub(crate) Option<Bound<'py, PyAny>>);
impl<'py> FromPyObject<'_, 'py> for Argument<'py> {
    type Error = PyErr;
    fn extract(value: Borrowed<'_, 'py, PyAny>) -> PyResult<Self> {
        Ok(Self(Some(value.to_owned())))
    }
}
pub(crate) fn boolean(value: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let py = value.py();
    if !value.is_exact_instance_of::<PyBool>() {
        return Err(crate::errors::builtin_message::<PyTypeError>(
            py,
            format!("{name} must be bool"),
        ));
    }
    value.extract()
}
pub(crate) fn text(value: &Bound<'_, PyAny>, name: &str, empty: bool) -> PyResult<String> {
    let py = value.py();
    let value = value
        .cast::<PyString>()
        .map_err(|_| {
            crate::errors::builtin_message::<PyTypeError>(py, format!("{name} must be str"))
        })?
        .to_str()?;
    if (!empty && value.is_empty()) || value.contains('\0') {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            format!(
                "{name} must {}contain no NUL",
                if empty { "" } else { "be nonempty and " }
            ),
        ));
    }
    Ok(value.to_owned())
}
pub(crate) fn optional_text(
    value: Option<&Bound<'_, PyAny>>,
    name: &str,
    empty: bool,
) -> PyResult<Option<String>> {
    value
        .filter(|value| !value.is_none())
        .map(|value| text(value, name, empty))
        .transpose()
}
pub(crate) fn path(value: &Bound<'_, PyAny>, name: &str) -> PyResult<PathBuf> {
    let py = value.py();
    if value.is_instance_of::<PyBytes>() {
        return Err(crate::errors::builtin_message::<PyTypeError>(
            py,
            format!("{name} must be str or os.PathLike; bare bytes are not accepted"),
        ));
    }
    // PyO3 invokes os.fspath exactly once and preserves the native OsString;
    // no manual separators, lossy UTF-8 conversion, expansion or realpath.
    let path = value.extract::<PathBuf>()?;
    if path.as_os_str().is_empty() || path.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            format!("{name} must be a nonempty filesystem path without NUL"),
        ));
    }
    Ok(path)
}
pub(crate) fn optional_path(
    value: Option<&Bound<'_, PyAny>>,
    name: &str,
) -> PyResult<Option<PathBuf>> {
    value
        .filter(|value| !value.is_none())
        .map(|value| path(value, name))
        .transpose()
}
pub(crate) fn positive(value: &Bound<'_, PyAny>, name: &str) -> PyResult<BigUint> {
    let py = value.py();
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(crate::errors::builtin_message::<PyTypeError>(
            py,
            format!("{name} must be a positive int; bool is not accepted"),
        ));
    }
    let value = crate::errors::integer_value(value)?;
    let value = value
        .to_biguint()
        .filter(|value| value != &BigUint::from(0u8))
        .ok_or_else(|| {
            crate::errors::builtin_message::<PyValueError>(
                py,
                format!("{name} must be greater than zero"),
            )
        })?;
    Ok(value)
}
pub(crate) fn timeout(value: Option<&Bound<'_, PyAny>>, name: &str) -> PyResult<Option<Duration>> {
    value
        .filter(|value| !value.is_none())
        .map(|value| {
            let py = value.py();
            let integer = positive(value, name)?;
            let seconds = u64::try_from(integer).map_err(|_| {
                crate::errors::builtin_error(
                    &value.py().get_type::<PyValueError>(),
                    &format!("{name} cannot be represented safely as a client duration"),
                )
            })?;
            let duration = Duration::from_secs(seconds);
            Instant::now().checked_add(duration).ok_or_else(|| {
                crate::errors::builtin_message::<PyValueError>(
                    py,
                    format!("{name} exceeds the platform's safe monotonic deadline range"),
                )
            })?;
            Ok(duration)
        })
        .transpose()
}
pub(crate) fn bytes_limit(value: &Bound<'_, PyAny>, name: &str, maximum: usize) -> PyResult<usize> {
    let py = value.py();
    let count = if let Ok(text) = value.cast::<PyString>() {
        size::parse_size(text.to_str()?).map_err(|error| {
            crate::errors::builtin_message::<PyValueError>(py, format!("{name}: {error}"))
        })?
    } else {
        usize::try_from(positive(value, name)?).map_err(|_| {
            crate::errors::builtin_message::<PyValueError>(py, format!("{name} is too large"))
        })?
    };
    if count == 0 || count > maximum {
        return Err(crate::errors::builtin_message::<PyValueError>(
            py,
            format!("{name} must represent 1..={maximum} bytes"),
        ));
    }
    Ok(count)
}

/// Immutable TLS configuration. Explicit PEM material is read and validated
/// once at construction, and native contexts are shared across connections.
#[pyclass(frozen, name = "SSL", module = "sqlaero")]
pub(crate) struct Ssl {
    mode: Option<Mode>,
    minimum: MinimumVersion,
    roots: Option<Vec<CertificateDer<'static>>>,
    identity: Option<Arc<CertifiedKey>>,
    server_name: Option<ServerName<'static>>,
    tcp: OnceLock<Option<Arc<rustls::ClientConfig>>>,
    unix: OnceLock<Option<Arc<rustls::ClientConfig>>>,
}
impl Ssl {
    pub(crate) fn mode_name(&self, unix: bool) -> &'static str {
        match self.mode.unwrap_or(if unix {
            Mode::Disabled
        } else {
            Mode::VerifyIdentity
        }) {
            Mode::Disabled => "DISABLED",
            Mode::Required => "REQUIRED",
            Mode::VerifyCa => "VERIFY_CA",
            Mode::VerifyIdentity => "VERIFY_IDENTITY",
        }
    }
    pub(crate) fn prepare(
        &self,
        py: Python<'_>,
        unix: bool,
        host: &str,
    ) -> PyResult<Option<sqlaero_core::session::connect::ConnectTls>> {
        let mode = self.mode.unwrap_or(if unix {
            Mode::Disabled
        } else {
            Mode::VerifyIdentity
        });
        if mode == Mode::Disabled && self.server_name.is_some() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "SSL server_name conflicts with DISABLED transport",
            ));
        }
        if unix && mode == Mode::VerifyIdentity && self.server_name.is_none() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "Unix VERIFY_IDENTITY requires an explicit SSL server_name",
            ));
        }
        let cached = if unix { &self.unix } else { &self.tcp };
        let config = match cached.get() {
            Some(value) => value.clone(),
            None => {
                let config = tls::client_config(
                    mode,
                    self.minimum,
                    self.roots.clone(),
                    self.identity.clone(),
                )
                .map_err(|error| {
                    crate::errors::builtin_message::<PyValueError>(py, error.to_string())
                })?;
                let _ = cached.set(config.clone());
                config
            }
        };
        let Some(config) = config else {
            return Ok(None);
        };
        let name = match &self.server_name {
            Some(name) => name.clone(),
            None if unix => ServerName::try_from("sqlaero.invalid")
                .map_err(|_| {
                    crate::errors::builtin_message::<PyValueError>(
                        py,
                        "invalid internal Unix TLS identity",
                    )
                })?
                .to_owned(),
            None => ServerName::try_from(host.to_owned()).map_err(|_| {
                crate::errors::builtin_message::<PyValueError>(
                    py,
                    "host is not a valid TLS DNS/IP identity; use SSL.server_name when appropriate",
                )
            })?,
        };
        Ok(Some(sqlaero_core::session::connect::ConnectTls {
            config,
            name,
        }))
    }
    pub(crate) fn default_config() -> Self {
        Self {
            mode: None,
            minimum: MinimumVersion::Tls12,
            roots: None,
            identity: None,
            server_name: None,
            tcp: OnceLock::new(),
            unix: OnceLock::new(),
        }
    }
}
#[pymethods]
impl Ssl {
    /// Prepare immutable TLS policy and PEM materials. All options are keyword
    /// only. mode=None uses VERIFY_IDENTITY for TCP and DISABLED for Unix.
    /// Paths accept str or os.PathLike, without expansion or canonicalization.
    /// Explicit CA roots replace system trust. No password or key is retained
    /// as a Python object, exposed by a property, or included in repr/errors.
    #[new]
    #[pyo3(signature=(*, mode=None, min_tls_version=None, ca_file=None, cert_file=None, key_file=None, key_password=None, server_name=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "the approved SSL public constructor has seven independent keyword options"
    )]
    fn new(
        py: Python<'_>,
        mode: Option<&Bound<'_, PyAny>>,
        min_tls_version: Option<&Bound<'_, PyAny>>,
        ca_file: Option<&Bound<'_, PyAny>>,
        cert_file: Option<&Bound<'_, PyAny>>,
        key_file: Option<&Bound<'_, PyAny>>,
        key_password: Option<&Bound<'_, PyAny>>,
        server_name: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mode = optional_text(mode, "mode", false)?
            .map(|value| value.parse::<Mode>())
            .transpose()
            .map_err(|error| {
                crate::errors::builtin_message::<PyValueError>(py, error.to_string())
            })?;
        let minimum = optional_text(min_tls_version, "min_tls_version", false)?
            .map(|value| value.parse::<MinimumVersion>())
            .transpose()
            .map_err(|error| crate::errors::builtin_message::<PyValueError>(py, error.to_string()))?
            .unwrap_or_default();
        let ca = optional_path(ca_file, "ca_file")?;
        let cert = optional_path(cert_file, "cert_file")?;
        let key = optional_path(key_file, "key_file")?;
        if cert.is_some() != key.is_some() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "cert_file and key_file must be provided together",
            ));
        }
        let password = optional_text(key_password, "key_password", false)?
            .map(|value| Zeroizing::new(value.into_bytes()));
        if password.is_some() && key.is_none() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "key_password requires key_file",
            ));
        }
        let name = optional_text(server_name, "server_name", false)?.map(|name| ServerName::try_from(name)
            .map_err(|_| crate::errors::builtin_message::<PyValueError>(py, "server_name must be a valid DNS name or IP without URL, port, path or wildcard"))).transpose()?;
        let roots = ca
            .map(|path| py.detach(|| std::fs::read(path)))
            .transpose()?
            .map(|pem| tls::certificates(&pem))
            .transpose()
            .map_err(|error| {
                crate::errors::builtin_message::<PyValueError>(py, error.to_string())
            })?;
        let identity = match (cert, key) {
            (Some(cert), Some(key)) => {
                let certificate = py.detach(|| std::fs::read(cert))?;
                let key = Zeroizing::new(py.detach(|| std::fs::read(key))?);
                let certificates = tls::certificates(&certificate).map_err(|error| {
                    crate::errors::builtin_message::<PyValueError>(py, error.to_string())
                })?;
                let key = tls::private_key(&key, password.as_deref().map(Vec::as_slice)).map_err(
                    |error| crate::errors::builtin_message::<PyValueError>(py, error.to_string()),
                )?;
                Some(
                    tls::signing_identity(certificates, key, &tls::provider()).map_err(
                        |error| {
                            crate::errors::builtin_message::<PyValueError>(py, error.to_string())
                        },
                    )?,
                )
            }
            _ => None,
        };
        let value = Self {
            mode,
            minimum,
            roots,
            identity,
            server_name: name,
            tcp: OnceLock::new(),
            unix: OnceLock::new(),
        };
        if let Some(mode) = mode {
            if mode == Mode::Disabled && value.server_name.is_some() {
                return Err(crate::errors::builtin_message::<PyValueError>(
                    py,
                    "SSL server_name conflicts with DISABLED transport",
                ));
            }
            let config =
                tls::client_config(mode, minimum, value.roots.clone(), value.identity.clone())
                    .map_err(|error| {
                        crate::errors::builtin_message::<PyValueError>(py, error.to_string())
                    })?;
            let _ = value.tcp.set(config.clone());
            let _ = value.unix.set(config);
        }
        Ok(value)
    }
    fn __repr__(&self) -> String {
        format!(
            "SSL(mode={})",
            match self.mode {
                None => "AUTO",
                Some(Mode::Disabled) => "DISABLED",
                Some(Mode::Required) => "REQUIRED",
                Some(Mode::VerifyCa) => "VERIFY_CA",
                Some(Mode::VerifyIdentity) => "VERIFY_IDENTITY",
            }
        )
    }
}

/// The immutable client timeout snapshot used by one whole command operation.
#[derive(Clone, Copy)]
pub(crate) struct Timeouts {
    pub(crate) connect: Option<Duration>,
    pub(crate) read: Option<Duration>,
    pub(crate) write: Option<Duration>,
    pub(crate) operation: Option<Duration>,
}
impl Timeouts {
    pub(crate) fn operation(&self) -> IoBudget {
        IoBudget::new(
            TimeoutKind::Operation,
            self.operation,
            self.read,
            self.write,
        )
    }
}
