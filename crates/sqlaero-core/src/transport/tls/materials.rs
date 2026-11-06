//! Filesystem material loading for one immutable TLS configuration snapshot.

use super::{ConfigError, MaterialError, ca_certificates, validate_anchors};
use rustls_pki_types::CertificateDer;
use std::borrow::Cow;
use std::collections::TryReserveError;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::Path;

/// A failed CA directory operation, preserving its exact member and root cause.
#[derive(Debug, thiserror::Error)]
pub enum DirectoryError<'a> {
    /// Native member or certificate storage could not be reserved.
    #[error("cannot allocate CA directory snapshot: {0}")]
    Allocation(#[from] TryReserveError),
    /// An enumeration, metadata, open, or read operation failed.
    #[error("ca_dir member {path:?}: {source}")]
    Io {
        /// The directory or member which failed.
        path: Cow<'a, Path>,
        /// The original operating-system failure.
        #[source]
        source: io::Error,
    },
    /// A candidate was not a nonempty, valid PEM certificate bundle.
    #[error("ca_dir member {path:?}: {source}")]
    Material {
        /// The candidate member which failed.
        path: Cow<'a, Path>,
        /// The non-secret PEM failure.
        #[source]
        source: MaterialError,
    },
    /// A candidate could not be prepared by the actual trust backend.
    #[error("ca_dir member {path:?}: {source}")]
    Trust {
        /// The candidate member which failed.
        path: Cow<'a, Path>,
        /// The native trust-material failure.
        #[source]
        source: ConfigError,
    },
    /// No eligible certificate files were found.
    #[error("ca_dir {0:?} contains no CA certificates")]
    Empty(Cow<'a, Path>),
}

fn io_error(path: &Path, source: io::Error) -> DirectoryError<'_> {
    DirectoryError::Io {
        path: Cow::Borrowed(path),
        source,
    }
}

fn candidate(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|suffix| {
            ["pem", "crt", "cer"]
                .iter()
                .any(|expected| suffix.eq_ignore_ascii_case(expected))
        })
}

/// Read an actual regular file, including normal file symlinks.
///
/// The opened object is checked again. This is not a directory sandbox or an
/// atomic filesystem snapshot. Unix nonblocking open also avoids waiting on a
/// FIFO substituted between the initial metadata check and the actual open.
///
/// # Errors
/// Returns the original IO error, or InvalidInput for nonregular objects.
pub fn regular_file(path: &Path) -> io::Result<Vec<u8>> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "material source must be a regular file",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let mut file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "opened material source is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Validate a complete explicit CA bundle using the selected native backend.
///
/// # Errors
/// Returns the backend's original configuration failure; no partial bundle is
/// accepted and no system roots are loaded.
pub fn validate_roots(certificates: &[CertificateDer<'static>]) -> Result<(), ConfigError> {
    validate_anchors(certificates).map(|_| ())
}

/// Copy an explicit root snapshot through checked native storage reservations.
///
/// # Errors
/// Returns the actual reservation failure without yielding a partial bundle.
pub fn clone_roots(
    certificates: &[CertificateDer<'static>],
) -> Result<Vec<CertificateDer<'static>>, TryReserveError> {
    let mut result = Vec::new();
    result.try_reserve_exact(certificates.len())?;
    for certificate in certificates {
        result.push(CertificateDer::from(super::owned_bytes(
            certificate.as_ref(),
        )?));
    }
    Ok(result)
}

/// Snapshot eligible direct CA members in native filename order.
///
/// Only ASCII case-insensitive pem/crt/cer extensions are selected. Directories
/// are skipped, including symlinks to directories; regular file symlinks may
/// point outside the selected directory. Every candidate must succeed before a
/// complete result is returned. Later filesystem changes do not alter it.
///
/// # Errors
/// Returns a path-specific IO or material error, or Empty when no CA is found.
pub fn directory_certificates(
    path: &Path,
) -> Result<Vec<CertificateDer<'static>>, DirectoryError<'_>> {
    let mut members = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| io_error(path, error))? {
        let member = entry.map_err(|error| io_error(path, error))?.path();
        members.try_reserve(1)?;
        members.push(member);
    }
    members.sort_unstable_by(|a, b| a.file_name().cmp(&b.file_name()));
    let mut result = Vec::new();
    for member in members {
        if !candidate(&member) {
            continue;
        }
        let metadata = match fs::metadata(&member) {
            Ok(metadata) => metadata,
            Err(source) => {
                return Err(DirectoryError::Io {
                    path: Cow::Owned(member),
                    source,
                });
            }
        };
        if metadata.is_dir() {
            continue;
        }
        let bytes = match regular_file(&member) {
            Ok(bytes) => bytes,
            Err(source) => {
                return Err(DirectoryError::Io {
                    path: Cow::Owned(member),
                    source,
                });
            }
        };
        let chain = match ca_certificates(&bytes) {
            Ok(chain) => chain,
            Err(MaterialError::Allocation(error)) => return Err(DirectoryError::Allocation(error)),
            Err(source) => {
                return Err(DirectoryError::Material {
                    path: Cow::Owned(member),
                    source,
                });
            }
        };
        match validate_roots(&chain) {
            Ok(()) => (),
            Err(ConfigError::Allocation(error)) => return Err(DirectoryError::Allocation(error)),
            Err(source) => {
                return Err(DirectoryError::Trust {
                    path: Cow::Owned(member),
                    source,
                });
            }
        }
        result.try_reserve(chain.len())?;
        result.extend(chain);
    }
    if result.is_empty() {
        return Err(DirectoryError::Empty(Cow::Borrowed(path)));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn suffix_selection_uses_entry_name_and_ascii_case() {
        for path in ["a.pem", "a.CRT", ".hidden.CeR"] {
            assert!(candidate(Path::new(path)));
        }
        for path in ["a", "a.txt", "a.pem.bak", "a.ＰＥＭ"] {
            assert!(!candidate(Path::new(path)));
        }
    }

    #[test]
    #[ignore = "requires the owned fixture CA certificate"]
    fn directory_is_ordered_complete_nonrecursive_and_follows_file_links() {
        let fixture = PathBuf::from(std::env::var_os("SQLAERO_TEST_MYSQL_ROOT").unwrap());
        let pem = fs::read(fixture.join("tls/ca.pem")).unwrap();
        let directory = std::env::temp_dir().join(format!(
            "sqlaero-d-ca-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        assert!(matches!(
            directory_certificates(&directory),
            Err(DirectoryError::Empty(_))
        ));
        fs::write(directory.join("b.CRT"), &pem).unwrap();
        fs::write(directory.join("a.pem"), &pem).unwrap();
        fs::write(directory.join(".hidden.cer"), &pem).unwrap();
        fs::write(directory.join("README"), b"not PEM").unwrap();
        fs::create_dir(directory.join("sub.pem")).unwrap();
        fs::write(directory.join("sub.pem/ignored.pem"), b"broken").unwrap();
        assert_eq!(directory_certificates(&directory).unwrap().len(), 3);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(fixture.join("tls/ca.pem"), directory.join("outside.PEM"))
                .unwrap();
            std::os::unix::fs::symlink(directory.join("sub.pem"), directory.join("linked-dir.CRT"))
                .unwrap();
            assert_eq!(directory_certificates(&directory).unwrap().len(), 4);
            std::os::unix::fs::symlink(directory.join("missing"), directory.join("broken.pem"))
                .unwrap();
            assert!(
                matches!(directory_certificates(&directory), Err(DirectoryError::Io { path, .. }) if path.ends_with("broken.pem"))
            );
            fs::remove_file(directory.join("broken.pem")).unwrap();
        }
        fs::write(directory.join("a.pem"), b"broken").unwrap();
        fs::write(directory.join("b.CRT"), b"also broken").unwrap();
        assert!(
            matches!(directory_certificates(&directory), Err(DirectoryError::Material { path, .. }) if path.ends_with("a.pem"))
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
