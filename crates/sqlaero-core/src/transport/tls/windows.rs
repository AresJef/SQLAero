//! Windows chain-policy adapter for the two MySQL CA verification modes.
//!
//! The operating system performs certificate and revocation validation. An
//! explicit CA bundle replaces system anchors, while VERIFY_CA omits only
//! hostname matching. All handles remain local to one verification call.

use rustls::{CertificateError, Error};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use std::ffi::c_void;
use std::io;
use std::ptr;
use windows_sys::Win32::Foundation::{
    CERT_E_CHAINING, CERT_E_CN_NO_MATCH, CERT_E_EXPIRED, CERT_E_INVALID_NAME, CERT_E_REVOKED,
    CERT_E_UNTRUSTEDROOT, CERT_E_WRONG_USAGE, CRYPT_E_REVOKED, FILETIME,
};
use windows_sys::Win32::Security::Cryptography::{
    AUTHTYPE_SERVER, CERT_CHAIN_CACHE_END_CERT, CERT_CHAIN_CONTEXT, CERT_CHAIN_ENGINE_CONFIG,
    CERT_CHAIN_PARA, CERT_CHAIN_POLICY_IGNORE_ALL_REV_UNKNOWN_FLAGS, CERT_CHAIN_POLICY_PARA,
    CERT_CHAIN_POLICY_SSL, CERT_CHAIN_POLICY_STATUS, CERT_CHAIN_REVOCATION_ACCUMULATIVE_TIMEOUT,
    CERT_CHAIN_REVOCATION_CHECK_END_CERT, CERT_CONTEXT, CERT_OCSP_RESPONSE_PROP_ID,
    CERT_SET_PROPERTY_IGNORE_PERSIST_ERROR_FLAG, CERT_STORE_ADD_ALWAYS,
    CERT_STORE_DEFER_CLOSE_UNTIL_LAST_FREE_FLAG, CERT_STORE_PROV_MEMORY, CRYPT_INTEGER_BLOB,
    CertAddEncodedCertificateToStore, CertCloseStore, CertCreateCertificateChainEngine,
    CertFreeCertificateChain, CertFreeCertificateChainEngine, CertFreeCertificateContext,
    CertGetCertificateChain, CertOpenStore, CertSetCertificateContextProperty,
    CertVerifyCertificateChainPolicy, HCERTCHAINENGINE, HCERTSTORE, HTTPSPolicyCallbackData,
    USAGE_MATCH_TYPE_AND, X509_ASN_ENCODING,
};

struct Store(HCERTSTORE);

impl Store {
    fn memory() -> Result<Self, Error> {
        // SAFETY: The memory-store provider takes no external configuration.
        // Deferred closure keeps individual certificate references valid.
        let handle = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_MEMORY,
                0,
                0,
                CERT_STORE_DEFER_CLOSE_UNTIL_LAST_FREE_FLAG,
                ptr::null(),
            )
        };
        if handle.is_null() {
            return Err(system_error());
        }
        Ok(Self(handle))
    }

    fn add(&self, data: &[u8]) -> Result<Certificate, Error> {
        let length = u32::try_from(data.len())
            .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
        let mut certificate = ptr::null_mut();
        // SAFETY: The store is owned, the exact DER slice remains readable,
        // and the output starts null. Windows copies the encoded certificate.
        let success = unsafe {
            CertAddEncodedCertificateToStore(
                self.0,
                X509_ASN_ENCODING,
                data.as_ptr(),
                length,
                CERT_STORE_ADD_ALWAYS,
                &mut certificate,
            )
        };
        if success == 0 || certificate.is_null() {
            return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
        }
        Ok(Certificate(certificate))
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: This is the one owned store handle returned by CertOpenStore.
        unsafe { CertCloseStore(self.0, 0) };
    }
}

struct Certificate(*mut CERT_CONTEXT);

impl Drop for Certificate {
    fn drop(&mut self) {
        // SAFETY: The successful add operation returned one owned reference.
        unsafe { CertFreeCertificateContext(self.0) };
    }
}

struct Engine {
    handle: HCERTCHAINENGINE,
    _roots: Store,
}

impl Engine {
    fn exclusive(anchors: &[CertificateDer<'_>]) -> Result<Self, Error> {
        let roots = Store::memory()?;
        for anchor in anchors {
            roots.add(anchor.as_ref())?;
        }
        let configuration = CERT_CHAIN_ENGINE_CONFIG {
            cbSize: size_of::<CERT_CHAIN_ENGINE_CONFIG>() as u32,
            hExclusiveRoot: roots.0,
            ..Default::default()
        };
        let mut handle = ptr::null_mut();
        // SAFETY: The initialized config and output are valid. The root store
        // is retained until after engine destruction, including error exits.
        if unsafe { CertCreateCertificateChainEngine(&configuration, &mut handle) } == 0
            || handle.is_null()
        {
            return Err(system_error());
        }
        Ok(Self {
            handle,
            _roots: roots,
        })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: The handle was created successfully and is freed once. The
        // owned root store is dropped only after this method returns.
        unsafe { CertFreeCertificateChainEngine(self.handle) };
    }
}

struct Chain(*mut CERT_CHAIN_CONTEXT);

impl Drop for Chain {
    fn drop(&mut self) {
        // SAFETY: This is the owned successful CertGetCertificateChain output.
        unsafe { CertFreeCertificateChain(self.0) };
    }
}

fn system_error() -> Error {
    Error::General(format!(
        "Windows certificate validation failed: {}",
        io::Error::last_os_error()
    ))
}

fn policy_error(code: u32) -> Error {
    let certificate = match code as i32 {
        CERT_E_EXPIRED => CertificateError::Expired,
        CERT_E_CN_NO_MATCH | CERT_E_INVALID_NAME => CertificateError::NotValidForName,
        CERT_E_UNTRUSTEDROOT | CERT_E_CHAINING => CertificateError::UnknownIssuer,
        CERT_E_WRONG_USAGE => CertificateError::InvalidPurpose,
        CRYPT_E_REVOKED | CERT_E_REVOKED => CertificateError::Revoked,
        _ => {
            return Error::General(format!(
                "Windows SSL certificate policy rejected the chain (HRESULT 0x{code:08X})"
            ));
        }
    };
    Error::InvalidCertificate(certificate)
}

/// Apply the native SSL policy without overriding its trust or revocation
/// decisions. A missing name disables only identity matching, as VERIFY_CA
/// requires. OS trust work is synchronous; the caller rechecks its original
/// deadline after returning, just as for the existing Apple adapter.
pub(super) fn verify(
    end: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    name: Option<&ServerName<'_>>,
    anchors: Option<&[CertificateDer<'static>]>,
    ocsp: &[u8],
    now: UnixTime,
) -> Result<(), Error> {
    let certificates = Store::memory()?;
    let end = certificates.add(end.as_ref())?;
    for intermediate in intermediates {
        certificates.add(intermediate.as_ref())?;
    }
    if !ocsp.is_empty() {
        let blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(ocsp.len())
                .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?,
            pbData: ocsp.as_ptr().cast_mut(),
        };
        // SAFETY: The certificate is owned and the exact initialized blob and
        // its OCSP bytes outlive the call. Windows copies this property.
        if unsafe {
            CertSetCertificateContextProperty(
                end.0,
                CERT_OCSP_RESPONSE_PROP_ID,
                CERT_SET_PROPERTY_IGNORE_PERSIST_ERROR_FLAG,
                (&blob as *const CRYPT_INTEGER_BLOB).cast(),
            )
        } == 0
        {
            return Err(system_error());
        }
    }
    let engine = anchors.map(Engine::exclusive).transpose()?;
    let intervals = now
        .as_secs()
        .checked_add(11_644_473_600)
        .and_then(|seconds| seconds.checked_mul(10_000_000))
        .ok_or_else(|| {
            Error::General("certificate verification time exceeds FILETIME".to_owned())
        })?;
    let time = FILETIME {
        dwLowDateTime: intervals as u32,
        dwHighDateTime: (intervals >> 32) as u32,
    };
    let mut usages = [c"1.3.6.1.5.5.7.3.1".as_ptr().cast_mut().cast()];
    let mut parameters = CERT_CHAIN_PARA {
        cbSize: size_of::<CERT_CHAIN_PARA>() as u32,
        dwUrlRetrievalTimeout: 10_000,
        ..Default::default()
    };
    parameters.RequestedUsage.dwType = USAGE_MATCH_TYPE_AND;
    parameters.RequestedUsage.Usage.cUsageIdentifier = 1;
    parameters.RequestedUsage.Usage.rgpszUsageIdentifier = usages.as_mut_ptr();
    let mut chain = ptr::null_mut();
    // Request the same leaf revocation work and soft-fail treatment for
    // unavailable revocation data as rustls-platform-verifier on Windows.
    // Positive revocation, wrong usage, invalid time and untrusted roots are
    // never ignored. Exclusive anchors never fall back to the system engine.
    let flags = CERT_CHAIN_REVOCATION_CHECK_END_CERT
        | CERT_CHAIN_REVOCATION_ACCUMULATIVE_TIMEOUT
        | CERT_CHAIN_CACHE_END_CERT;
    // SAFETY: All referenced stores, certificate, usage strings, time and
    // parameters remain alive until verification ends. The output starts null.
    if unsafe {
        CertGetCertificateChain(
            engine
                .as_ref()
                .map_or(ptr::null_mut(), |engine| engine.handle),
            end.0,
            &time,
            certificates.0,
            &parameters,
            flags,
            ptr::null(),
            &mut chain,
        )
    } == 0
        || chain.is_null()
    {
        return Err(system_error());
    }
    let chain = Chain(chain);
    let mut hostname: Option<Vec<u16>> = name.map(|name| {
        name.to_str()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    });
    let mut extra = HTTPSPolicyCallbackData {
        dwAuthType: AUTHTYPE_SERVER,
        pwszServerName: hostname
            .as_mut()
            .map_or(ptr::null_mut(), |name| name.as_mut_ptr()),
        ..Default::default()
    };
    extra.Anonymous.cbSize = size_of::<HTTPSPolicyCallbackData>() as u32;
    let policy = CERT_CHAIN_POLICY_PARA {
        cbSize: size_of::<CERT_CHAIN_POLICY_PARA>() as u32,
        dwFlags: CERT_CHAIN_POLICY_IGNORE_ALL_REV_UNKNOWN_FLAGS,
        pvExtraPolicyPara: (&mut extra as *mut HTTPSPolicyCallbackData).cast::<c_void>(),
    };
    let mut status = CERT_CHAIN_POLICY_STATUS {
        cbSize: size_of::<CERT_CHAIN_POLICY_STATUS>() as u32,
        ..Default::default()
    };
    // SAFETY: The chain and all policy/name storage outlive the call. The
    // status structure has its required size and valid writable storage.
    if unsafe {
        CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL, chain.0, &policy, &mut status)
    } == 0
    {
        return Err(system_error());
    }
    if status.dwError != 0 {
        return Err(policy_error(status.dwError));
    }
    Ok(())
}
