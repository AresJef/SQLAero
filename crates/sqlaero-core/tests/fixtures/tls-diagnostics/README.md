# Public diagnostic certificates

These signed certificates contain public test data only. The first two inspect
X.509 fields, not trust or the current date. No private
key is stored in this directory or committed with these fixtures.

They were generated with OpenSSL 4 from the isolated D fixture's server CSR and
signing key. Both use serial `0102030405060708090a0b0c0d0e0f1011121314` and a
3,650-day validity window. `no-extensions.pem` disables subjectKeyIdentifier and
has no extensions. `rich.pem` uses the subject
`/C=US/O=SQLAero, Diagnostics/OU=Team+CN=db.example.invalid`, basicConstraints
`CA:FALSE`, and this subjectAltName sequence:

```text
DNS:db.example.invalid,DNS:*.internal.invalid,DNS:db.example.invalid,
IP:192.0.2.10,IP:2001:db8::1,IP:192.0.2.10
```

`anchors.pem` is a CA:TRUE self-signed certificate reissued from the D fixture's
CA using the same serial and 3,650-day window. The optional append-trust probe
adds it as an unrelated private CA and verifies that native public-CA trust
still works. It is never installed in the operating system trust store.

Tests also change exact bytes of an owned copy to exercise deterministic local
content failures. Such copies are never used as trusted TLS identities.
