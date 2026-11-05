//! Strict boundary configuration, shared path parsing and reusable TLS material.

use num_bigint::BigUint;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyInt, PyString};
use rustls::sign::CertifiedKey;
use rustls_pki_types::{CertificateDer, ServerName};
use sqlaero_core::{
    config::size,
    transport::{
        deadline::{IoBudget, TimeoutKind},
        tls::{self, Mode, Version},
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
    value.filter(|value| !value.is_none()).map(|value| {
        let py = value.py();
        let duration = if value.is_exact_instance_of::<PyInt>() {
            let seconds = u64::try_from(positive(value, name)?).map_err(|_| {
                crate::errors::builtin_message::<PyValueError>(py, format!("{name} cannot be represented safely as a client duration"))
            })?;
            Duration::from_secs(seconds)
        } else if value.is_exact_instance_of::<PyFloat>() {
            let seconds = value.cast::<PyFloat>()?.value();
            if !seconds.is_finite() || seconds < 1e-9 {
                return Err(crate::errors::builtin_message::<PyValueError>(py,
                    format!("{name} must be finite and at least 1 nanosecond")));
            }
            Duration::try_from_secs_f64(seconds).map_err(|_| {
                crate::errors::builtin_message::<PyValueError>(py, format!("{name} cannot be represented safely as a client duration"))
            })?
        } else {
            return Err(crate::errors::builtin_message::<PyTypeError>(py,
                format!("{name} must be an exact built-in int or float, or None; bool and numeric subclasses are not accepted")));
        };
        Instant::now().checked_add(duration).ok_or_else(|| {
            crate::errors::builtin_message::<PyValueError>(py, format!("{name} exceeds the platform's safe monotonic deadline range"))
        })?;
        Ok(duration)
    }).transpose()
}

/// Materialize the saved duration without retaining the original Python input.
/// Whole seconds use int, subsecond values use float, and disabled uses None.
pub(crate) fn timeout_object<'py>(
    py: Python<'py>,
    value: Option<Duration>,
) -> PyResult<Bound<'py, PyAny>> {
    let Some(value) = value else {
        return Ok(py.None().into_bound(py));
    };
    // SAFETY: both public CPython constructors return a new owned reference or
    // their actual allocation error. No source Python object is retained.
    unsafe {
        let pointer = if value.subsec_nanos() == 0 {
            pyo3::ffi::PyLong_FromUnsignedLongLong(value.as_secs())
        } else {
            pyo3::ffi::PyFloat_FromDouble(value.as_secs_f64())
        };
        Bound::from_owned_ptr_or_err(py, pointer)
    }
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

pub(crate) fn public_key(
    value: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<sqlaero_core::session::connect::PublicKeySource>> {
    use sqlaero_core::session::connect::PublicKeySource;
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };
    let py = value.py();
    if let Ok(raw) = value.cast::<PyBytes>() {
        let raw = raw.as_bytes();
        if raw.is_empty() {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "server_public_key must contain nonempty SPKI PEM bytes",
            ));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(raw.len())
            .map_err(|_| crate::errors::memory_error(py))?;
        bytes.extend_from_slice(raw);
        return Ok(Some(PublicKeySource::Pem(bytes)));
    }
    path(value, "server_public_key")
        .map(PublicKeySource::Path)
        .map(Some)
}

fn provided(value: Option<&Bound<'_, PyAny>>) -> bool {
    value.is_some_and(|value| !value.is_none())
}

fn pem_data(value: Option<&Bound<'_, PyAny>>, name: &str) -> PyResult<Option<Zeroizing<Vec<u8>>>> {
    let Some(value) = value.filter(|value| !value.is_none()) else {
        return Ok(None);
    };
    let py = value.py();
    let raw = if let Ok(value) = value.cast::<PyBytes>() {
        value.as_bytes()
    } else if let Ok(value) = value.cast::<PyString>() {
        value.to_str()?.as_bytes()
    } else {
        return Err(crate::errors::builtin_format::<PyTypeError>(
            py,
            format_args!("{name} must be str, bytes or None"),
        ));
    };
    if raw.is_empty() {
        return Err(crate::errors::builtin_format::<PyValueError>(
            py,
            format_args!("{name} must contain nonempty PEM material"),
        ));
    }
    let mut owned = Zeroizing::new(Vec::new());
    owned
        .try_reserve_exact(raw.len())
        .map_err(|_| crate::errors::memory_error(py))?;
    owned.extend_from_slice(raw);
    Ok(Some(owned))
}

fn tls_material_error(py: Python<'_>, name: &str, error: tls::MaterialError) -> PyErr {
    match error {
        tls::MaterialError::Allocation(_) => crate::errors::memory_error(py),
        error => crate::errors::builtin_format::<PyValueError>(py, format_args!("{name}: {error}")),
    }
}
fn tls_config_error(py: Python<'_>, error: tls::ConfigError) -> PyErr {
    match error {
        tls::ConfigError::Allocation(_) => crate::errors::memory_error(py),
        error => crate::errors::builtin_format::<PyValueError>(py, format_args!("{error}")),
    }
}
fn tls_directory_error(py: Python<'_>, error: tls::materials::DirectoryError<'_>) -> PyErr {
    use tls::materials::DirectoryError;
    match error {
        DirectoryError::Allocation(_) => crate::errors::memory_error(py),
        DirectoryError::Io { path, source } => {
            if source.kind() == std::io::ErrorKind::OutOfMemory {
                return crate::errors::memory_error(py);
            }
            let primary: PyErr = source.into();
            crate::errors::note_format(py, &primary, format_args!("ca_dir member {path:?}"));
            primary
        }
        DirectoryError::Material {
            source: tls::MaterialError::Allocation(_),
            ..
        }
        | DirectoryError::Trust {
            source: tls::ConfigError::Allocation(_),
            ..
        } => crate::errors::memory_error(py),
        error => crate::errors::builtin_format::<PyValueError>(py, format_args!("{error}")),
    }
}
struct Material {
    bytes: Zeroizing<Vec<u8>>,
    path: Option<PathBuf>,
    name: &'static str,
}
impl Material {
    fn failure(&self, py: Python<'_>, error: PyErr) -> PyErr {
        match &self.path {
            Some(path) => crate::errors::note_format(
                py,
                &error,
                format_args!("while preparing {} from {path:?}", self.name),
            ),
            None => crate::errors::note_format(
                py,
                &error,
                format_args!("while preparing {}", self.name),
            ),
        }
        error
    }
}

fn material_bytes(
    py: Python<'_>,
    path: Option<PathBuf>,
    data: Option<Zeroizing<Vec<u8>>>,
    name: &'static str,
) -> PyResult<Option<Material>> {
    match path {
        Some(path) => {
            let bytes = py
                .detach(|| tls::materials::regular_file(&path))
                .map_err(|source| {
                    if source.kind() == std::io::ErrorKind::OutOfMemory {
                        return crate::errors::memory_error(py);
                    }
                    let primary: PyErr = source.into();
                    crate::errors::note_format(py, &primary, format_args!("{name}: {path:?}"));
                    primary
                })?;
            Ok(Some(Material {
                bytes: Zeroizing::new(bytes),
                path: Some(path),
                name,
            }))
        }
        None => Ok(data.map(|bytes| Material {
            bytes,
            path: None,
            name,
        })),
    }
}

/// Immutable TLS configuration. Explicit PEM material is read and validated
/// once at construction, and native contexts are shared across connections.
#[pyclass(frozen, name = "SSL", module = "sqlaero")]
pub(crate) struct Ssl {
    mode: Option<Mode>,
    minimum: Version,
    maximum: Option<Version>,
    ca_mode: tls::CaMode,
    minimum_value: OnceLock<Py<PyString>>,
    maximum_value: OnceLock<Py<PyString>>,
    roots: Option<Vec<CertificateDer<'static>>>,
    identity: Option<Arc<CertifiedKey>>,
    server_name: Option<ServerName<'static>>,
    tcp: OnceLock<Option<Arc<rustls::ClientConfig>>>,
    unix: OnceLock<Option<Arc<rustls::ClientConfig>>>,
}
impl Ssl {
    pub(crate) fn resolved_mode(&self, unix: bool) -> Mode {
        self.mode.unwrap_or(if unix {
            Mode::Disabled
        } else {
            Mode::VerifyIdentity
        })
    }
    pub(crate) fn minimum(&self) -> Version {
        self.minimum
    }
    pub(crate) fn maximum(&self) -> Option<Version> {
        self.maximum
    }
    pub(crate) fn has_explicit_server_name(&self) -> bool {
        self.server_name.is_some()
    }

    fn native_config(
        &self,
        py: Python<'_>,
        mode: Mode,
        unix: bool,
    ) -> PyResult<Option<Arc<rustls::ClientConfig>>> {
        let roots = self.roots.as_deref();
        let minimum = self.minimum;
        let maximum = self.maximum;
        let ca_mode = self.ca_mode;
        let identity = self.identity.clone();
        let unnamed_unix = unix && self.server_name.is_none();
        py.detach(|| {
            let roots = roots
                .map(tls::materials::clone_roots)
                .transpose()
                .map_err(tls::ConfigError::Allocation)?;
            let mut config = tls::client_config(mode, minimum, maximum, ca_mode, roots, identity)?;
            if unnamed_unix && let Some(config) = config.as_mut() {
                Arc::make_mut(config).enable_sni = false;
            }
            Ok(config)
        })
        .map_err(|error| tls_config_error(py, error))
    }

    fn version_value<'py>(&self, py: Python<'py>, maximum: bool) -> PyResult<Bound<'py, PyAny>> {
        let (version, cache) = if maximum {
            (self.maximum, &self.maximum_value)
        } else {
            (Some(self.minimum), &self.minimum_value)
        };
        let Some(version) = version else {
            return Ok(py.None().into_bound(py));
        };
        if let Some(value) = cache.get() {
            return Ok(value.clone_ref(py).into_bound(py).into_any());
        }
        let value = PyString::from_bytes(py, version.as_str().as_bytes())?.unbind();
        let _ = cache.set(value);
        cache
            .get()
            .map(|value| value.clone_ref(py).into_bound(py).into_any())
            .ok_or_else(|| {
                crate::errors::client(
                    py,
                    sqlaero_core::error::ClientErrorCode::InternalState,
                    "SSL version representation was not published",
                )
            })
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
                let config = self.native_config(py, mode, unix)?;
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
            minimum: self.minimum,
            maximum: self.maximum,
            config,
            name,
        }))
    }
    pub(crate) fn default_config() -> Self {
        Self {
            mode: None,
            minimum: Version::Tls12,
            maximum: None,
            ca_mode: tls::CaMode::Replace,
            minimum_value: OnceLock::new(),
            maximum_value: OnceLock::new(),
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
    /// Explicit CA roots replace system trust unless ca_mode="append".
    /// ca_file/ca_dir/ca_data are exclusive; each cert/key source is exclusive.
    /// PEM data accepts str or bytes. min/max_tls_version bound TLS 1.2/1.3.
    /// CA directories select direct pem/crt/cer members without recursion.
    /// No password or key is retained
    /// as a Python object, exposed by a property, or included in repr/errors.
    #[new]
    #[pyo3(signature=(*, mode=None, min_tls_version=None, max_tls_version=None, ca_file=None, ca_dir=None, ca_data=None, ca_mode=None, cert_file=None, cert_data=None, key_file=None, key_data=None, key_password=None, server_name=None))]
    #[expect(
        clippy::too_many_arguments,
        reason = "the final SSL constructor has explicit independent keyword options"
    )]
    fn new(
        py: Python<'_>,
        mode: Option<&Bound<'_, PyAny>>,
        min_tls_version: Option<&Bound<'_, PyAny>>,
        max_tls_version: Option<&Bound<'_, PyAny>>,
        ca_file: Option<&Bound<'_, PyAny>>,
        ca_dir: Option<&Bound<'_, PyAny>>,
        ca_data: Option<&Bound<'_, PyAny>>,
        ca_mode: Option<&Bound<'_, PyAny>>,
        cert_file: Option<&Bound<'_, PyAny>>,
        cert_data: Option<&Bound<'_, PyAny>>,
        key_file: Option<&Bound<'_, PyAny>>,
        key_data: Option<&Bound<'_, PyAny>>,
        key_password: Option<&Bound<'_, PyAny>>,
        server_name: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mode = optional_text(mode, "mode", false)?
            .map(|value| value.parse::<Mode>())
            .transpose()
            .map_err(|error| {
                crate::errors::builtin_message::<PyValueError>(py, error.to_string())
            })?;
        let parse_version =
            |value: Option<&Bound<'_, PyAny>>, name: &str| -> PyResult<Option<Version>> {
                optional_text(value, name, false)?
                    .map(|value| {
                        value.parse::<Version>().map_err(|error| {
                            crate::errors::builtin_format::<PyValueError>(
                                py,
                                format_args!("{name}={value:?}: {error}"),
                            )
                        })
                    })
                    .transpose()
            };
        let minimum = parse_version(min_tls_version, "min_tls_version")?.unwrap_or_default();
        let maximum = parse_version(max_tls_version, "max_tls_version")?;
        tls::validate_range(minimum, maximum).map_err(|error| {
            crate::errors::builtin_format::<PyValueError>(
                py,
                format_args!(
                    "min_tls_version={:?}, max_tls_version={:?}: {error}",
                    minimum.as_str(),
                    maximum.map(Version::as_str)
                ),
            )
        })?;
        let ca_mode = optional_text(ca_mode, "ca_mode", false)?
            .map(|value| {
                value.parse::<tls::CaMode>().map_err(|error| {
                    crate::errors::builtin_format::<PyValueError>(
                        py,
                        format_args!("ca_mode={value:?}: {error}"),
                    )
                })
            })
            .transpose()?
            .unwrap_or_default();

        // Presence conflicts precede path callbacks and I/O; no precedence is chosen.
        let ca_count = [ca_file, ca_dir, ca_data]
            .into_iter()
            .filter(|value| provided(*value))
            .count();
        if ca_count > 1 {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "ca_file, ca_dir and ca_data are mutually exclusive",
            ));
        }
        if provided(cert_file) && provided(cert_data) {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "cert_file and cert_data are mutually exclusive",
            ));
        }
        if provided(key_file) && provided(key_data) {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "key_file and key_data are mutually exclusive",
            ));
        }
        let cert_present = provided(cert_file) || provided(cert_data);
        let key_present = provided(key_file) || provided(key_data);

        // Validate all approved material types before parsing any material.
        let ca_file = optional_path(ca_file, "ca_file")?;
        let ca_dir = optional_path(ca_dir, "ca_dir")?;
        let ca_data = pem_data(ca_data, "ca_data")?;
        let cert_name = if provided(cert_file) {
            "cert_file"
        } else {
            "cert_data"
        };
        let key_name = if provided(key_file) {
            "key_file"
        } else {
            "key_data"
        };
        // Save these names before shadowing the actual arguments.
        let cert_file = optional_path(cert_file, "cert_file")?;
        let cert_data = pem_data(cert_data, "cert_data")?;
        let key_file = optional_path(key_file, "key_file")?;
        let key_data = pem_data(key_data, "key_data")?;
        if cert_present != key_present {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "client certificate (cert_file/cert_data) and private key (key_file/key_data) must be provided together",
            ));
        }
        let password = optional_text(key_password, "key_password", false)?
            .map(|value| Zeroizing::new(value.into_bytes()));
        if password.is_some() && !key_present {
            return Err(crate::errors::builtin_message::<PyValueError>(
                py,
                "key_password requires key_file or key_data",
            ));
        }

        let roots = if let Some(path) = ca_dir {
            Some(
                py.detach(|| tls::materials::directory_certificates(&path))
                    .map_err(|error| tls_directory_error(py, error))?,
            )
        } else {
            let source_name = if ca_file.is_some() {
                "ca_file"
            } else {
                "ca_data"
            };
            material_bytes(py, ca_file, ca_data, source_name)?
                .map(|material| {
                    let roots = tls::ca_certificates(&material.bytes).map_err(|error| {
                        material.failure(py, tls_material_error(py, material.name, error))
                    })?;
                    py.detach(|| tls::materials::validate_roots(&roots))
                        .map_err(|error| material.failure(py, tls_config_error(py, error)))?;
                    Ok::<_, PyErr>(roots)
                })
                .transpose()?
        };
        let certificate = material_bytes(py, cert_file, cert_data, cert_name)?;
        let key = material_bytes(py, key_file, key_data, key_name)?;
        let identity = match (certificate, key) {
            (Some(certificate), Some(key)) => {
                let chain = tls::certificates(&certificate.bytes).map_err(|error| {
                    certificate.failure(py, tls_material_error(py, certificate.name, error))
                })?;
                let parsed_key = py
                    .detach(|| tls::private_key(&key.bytes, password.as_deref().map(Vec::as_slice)))
                    .map_err(|error| key.failure(py, tls_material_error(py, key.name, error)))?;
                Some(
                    tls::signing_identity(chain, parsed_key, &tls::provider()).map_err(
                        |error| {
                            certificate.failure(
                                py,
                                key.failure(
                                    py,
                                    tls_material_error(py, "client certificate/private key", error),
                                ),
                            )
                        },
                    )?,
                )
            }
            (None, None) => None,
            _ => {
                return Err(crate::errors::builtin_message::<PyValueError>(
                    py,
                    "client identity sources are incomplete",
                ));
            }
        };
        let name = optional_text(server_name, "server_name", false)?.map(|name| ServerName::try_from(name)
            .map_err(|_| crate::errors::builtin_message::<PyValueError>(py, "server_name must be a valid DNS name or IP without URL, port, path or wildcard"))).transpose()?;
        let value = Self {
            mode,
            minimum,
            maximum,
            ca_mode,
            minimum_value: OnceLock::new(),
            maximum_value: OnceLock::new(),
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
            let config = value.native_config(py, mode, false)?;
            let _ = value.tcp.set(config.clone());
            if value.server_name.is_some() || config.is_none() {
                let _ = value.unix.set(config);
            }
        }
        Ok(value)
    }
    /// The normalized configured minimum TLS version; limits apply only with TLS.
    #[getter]
    fn min_tls_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.version_value(py, false)
    }
    /// The extra configured maximum TLS version, or None; this is configuration.
    #[getter]
    fn max_tls_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.version_value(py, true)
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
