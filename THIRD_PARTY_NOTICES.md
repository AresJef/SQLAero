# Third-party notices

This native build uses the locked packages below. Original notices and license
texts are preserved under `licenses/third-party/`. This inventory describes the
union of the locked runtime/build graphs for all four native release targets;
it is updated when the shipped graphs change.
The package license expression does not replace its original copyright notices.

SQLAero carries a narrowly patched jiter 0.17.0 at `third-party/jiter/`.
Its upstream MIT license and platform scanners are preserved. The local changes
add fallible work checkpoints used only by the Python result boundary; provenance,
scope and upgrade instructions are in `third-party/jiter/SQLAERO_PATCH.md`.

| Package | Version | Declared license | Original texts |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [LICENSE-0BSD](licenses/third-party/adler2-2.0.1/LICENSE-0BSD), [LICENSE-APACHE](licenses/third-party/adler2-2.0.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/adler2-2.0.1/LICENSE-MIT) |
| aead | 0.6.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/aead-0.6.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/aead-0.6.1/LICENSE-MIT) |
| aes | 0.9.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/aes-0.9.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/aes-0.9.3/LICENSE-MIT) |
| aes-gcm | 0.11.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/aes-gcm-0.11.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/aes-gcm-0.11.1/LICENSE-MIT) |
| ahash | 0.8.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/ahash-0.8.12/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/ahash-0.8.12/LICENSE-MIT) |
| aho-corasick | 1.1.5 | Unlicense OR MIT | [COPYING](licenses/third-party/aho-corasick-1.1.5/COPYING), [LICENSE-MIT](licenses/third-party/aho-corasick-1.1.5/LICENSE-MIT), [UNLICENSE](licenses/third-party/aho-corasick-1.1.5/UNLICENSE) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/autocfg-1.5.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/autocfg-1.5.1/LICENSE-MIT) |
| aws-lc-rs | 1.18.0 | ISC AND (Apache-2.0 OR ISC) | [LICENSE](licenses/third-party/aws-lc-rs-1.18.0/LICENSE) |
| aws-lc-sys | 0.44.0 | ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0) | [LICENSE](licenses/third-party/aws-lc-sys-0.44.0/LICENSE), [LICENSE](licenses/third-party/aws-lc-sys-0.44.0/aws-lc/LICENSE) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/base64-0.22.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/base64-0.22.1/LICENSE-MIT) |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/base64ct-1.8.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/base64ct-1.8.3/LICENSE-MIT) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/bitflags-2.13.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/bitflags-2.13.2/LICENSE-MIT) |
| bitvec | 1.1.1 | MIT | [LICENSE.txt](licenses/third-party/bitvec-1.1.1/LICENSE.txt) |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/block-buffer-0.12.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/block-buffer-0.12.1/LICENSE-MIT) |
| block-padding | 0.4.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/block-padding-0.4.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/block-padding-0.4.2/LICENSE-MIT) |
| btoi | 0.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/btoi-0.5.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/btoi-0.5.0/LICENSE-MIT) |
| byteorder | 1.5.0 | Unlicense OR MIT | [COPYING](licenses/third-party/byteorder-1.5.0/COPYING), [LICENSE-MIT](licenses/third-party/byteorder-1.5.0/LICENSE-MIT), [UNLICENSE](licenses/third-party/byteorder-1.5.0/UNLICENSE) |
| bytes | 1.12.1 | MIT | [LICENSE](licenses/third-party/bytes-1.12.1/LICENSE) |
| cbc | 0.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cbc-0.2.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cbc-0.2.1/LICENSE-MIT) |
| cc | 1.10.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cc-1.10.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cc-1.10.0/LICENSE-MIT) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cfg-if-1.0.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cfg-if-1.0.5/LICENSE-MIT) |
| cipher | 0.5.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cipher-0.5.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cipher-0.5.2/LICENSE-MIT) |
| cmake | 0.1.58 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cmake-0.1.58/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cmake-0.1.58/LICENSE-MIT) |
| cmov | 0.5.4 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/cmov-0.5.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cmov-0.5.4/LICENSE-MIT) |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/const-oid-0.10.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/const-oid-0.10.2/LICENSE-MIT) |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/core-foundation-0.10.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/core-foundation-0.10.1/LICENSE-MIT) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/core-foundation-sys-0.8.7/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/core-foundation-sys-0.8.7/LICENSE-MIT) |
| core_detect | 1.0.0 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/third-party/core_detect-1.0.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/core_detect-1.0.0/LICENSE-MIT) |
| cpubits | 0.1.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cpubits-0.1.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cpubits-0.1.1/LICENSE-MIT) |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/cpufeatures-0.3.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/cpufeatures-0.3.1/LICENSE-MIT) |
| crc32fast | 1.5.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crc32fast-1.5.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crc32fast-1.5.3/LICENSE-MIT) |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/crypto-common-0.2.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/crypto-common-0.2.2/LICENSE-MIT) |
| ctr | 0.10.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/ctr-0.10.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/ctr-0.10.1/LICENSE-MIT) |
| ctutils | 0.4.5 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/ctutils-0.4.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/ctutils-0.4.5/LICENSE-MIT) |
| der | 0.8.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/der-0.8.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/der-0.8.2/LICENSE-MIT) |
| digest | 0.11.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/digest-0.11.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/digest-0.11.3/LICENSE-MIT) |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [LICENSE](licenses/third-party/dunce-1.0.5/LICENSE) |
| encoding_rs | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | [COPYRIGHT](licenses/third-party/encoding_rs-0.8.42/COPYRIGHT), [LICENSE-APACHE](licenses/third-party/encoding_rs-0.8.42/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/encoding_rs-0.8.42/LICENSE-MIT), [LICENSE-WHATWG](licenses/third-party/encoding_rs-0.8.42/LICENSE-WHATWG) |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/find-msvc-tools-0.1.15/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/find-msvc-tools-0.1.15/LICENSE-MIT) |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/flate2-1.1.10/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/flate2-1.1.10/LICENSE-MIT) |
| fs_extra | 1.3.0 | MIT | [LICENSE](licenses/third-party/fs_extra-1.3.0/LICENSE) |
| funty | 2.0.0 | MIT | [LICENSE.txt](licenses/third-party/funty-2.0.0/LICENSE.txt) |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/getrandom-0.2.17/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/getrandom-0.2.17/LICENSE-MIT) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/getrandom-0.3.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/getrandom-0.3.4/LICENSE-MIT) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/getrandom-0.4.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/getrandom-0.4.3/LICENSE-MIT) |
| ghash | 0.6.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/ghash-0.6.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/ghash-0.6.0/LICENSE-MIT) |
| heck | 0.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/heck-0.5.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/heck-0.5.0/LICENSE-MIT) |
| hex | 0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/hex-0.4.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/hex-0.4.3/LICENSE-MIT) |
| hmac | 0.13.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/hmac-0.13.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/hmac-0.13.0/LICENSE-MIT) |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/hybrid-array-0.4.15/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/hybrid-array-0.4.15/LICENSE-MIT) |
| inout | 0.2.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/inout-0.2.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/inout-0.2.5/LICENSE-MIT) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/itoa-1.0.18/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/itoa-1.0.18/LICENSE-MIT) |
| jiter | 0.17.0 | MIT | [LICENSE](licenses/third-party/jiter-0.17.0/LICENSE) |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/jobserver-0.1.35/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/jobserver-0.1.35/LICENSE-MIT) |
| lexical-parse-float | 1.0.6 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/third-party/lexical-parse-float-1.0.6/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/lexical-parse-float-1.0.6/LICENSE-MIT), [LICENSE.md](licenses/third-party/lexical-parse-float-1.0.6/LICENSE.md) |
| lexical-parse-integer | 1.0.6 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/third-party/lexical-parse-integer-1.0.6/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/lexical-parse-integer-1.0.6/LICENSE-MIT), [LICENSE.md](licenses/third-party/lexical-parse-integer-1.0.6/LICENSE.md) |
| lexical-util | 1.0.7 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/third-party/lexical-util-1.0.7/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/lexical-util-1.0.7/LICENSE-MIT) |
| libc | 0.2.191 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/libc-0.2.191/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/libc-0.2.191/LICENSE-MIT) |
| log | 0.4.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/log-0.4.34/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/log-0.4.34/LICENSE-MIT) |
| memchr | 2.8.3 | Unlicense OR MIT | [COPYING](licenses/third-party/memchr-2.8.3/COPYING), [LICENSE-MIT](licenses/third-party/memchr-2.8.3/LICENSE-MIT), [UNLICENSE](licenses/third-party/memchr-2.8.3/UNLICENSE) |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | [LICENSE](licenses/third-party/miniz_oxide-0.9.1/LICENSE), [LICENSE-APACHE.md](licenses/third-party/miniz_oxide-0.9.1/LICENSE-APACHE.md), [LICENSE-MIT.md](licenses/third-party/miniz_oxide-0.9.1/LICENSE-MIT.md), [LICENSE-ZLIB.md](licenses/third-party/miniz_oxide-0.9.1/LICENSE-ZLIB.md) |
| mio | 1.2.4 | MIT | [LICENSE](licenses/third-party/mio-1.2.4/LICENSE) |
| multiversion_no_op | 1.0.0 | Apache-2.0 OR MIT | [COPYRIGHT](licenses/third-party/multiversion_no_op-1.0.0/COPYRIGHT), [LICENSE-APACHE](licenses/third-party/multiversion_no_op-1.0.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/multiversion_no_op-1.0.0/LICENSE-MIT) |
| mysql_common | 0.38.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/mysql_common-0.38.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/mysql_common-0.38.2/LICENSE-MIT) |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/num-bigint-0.4.8/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/num-bigint-0.4.8/LICENSE-MIT) |
| num-bigint | 0.5.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/num-bigint-0.5.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/num-bigint-0.5.1/LICENSE-MIT) |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/num-integer-0.1.47/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/num-integer-0.1.47/LICENSE-MIT) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/num-traits-0.2.19/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/num-traits-0.2.19/LICENSE-MIT) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/once_cell-1.21.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/once_cell-1.21.4/LICENSE-MIT) |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/openssl-probe-0.2.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/openssl-probe-0.2.1/LICENSE-MIT) |
| pbkdf2 | 0.13.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pbkdf2-0.13.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pbkdf2-0.13.0/LICENSE-MIT) |
| pem-rfc7468 | 1.0.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/pem-rfc7468-1.0.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pem-rfc7468-1.0.0/LICENSE-MIT) |
| pkcs5 | 0.8.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/pkcs5-0.8.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pkcs5-0.8.1/LICENSE-MIT) |
| pkcs8 | 0.11.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/pkcs8-0.11.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pkcs8-0.11.0/LICENSE-MIT) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pkg-config-0.3.34/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pkg-config-0.3.34/LICENSE-MIT) |
| polyval | 0.7.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/polyval-0.7.3/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/polyval-0.7.3/LICENSE-MIT) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/proc-macro2-1.0.107/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/proc-macro2-1.0.107/LICENSE-MIT) |
| pyo3 | 0.29.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pyo3-0.29.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pyo3-0.29.2/LICENSE-MIT) |
| pyo3-build-config | 0.29.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pyo3-build-config-0.29.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pyo3-build-config-0.29.2/LICENSE-MIT) |
| pyo3-ffi | 0.29.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pyo3-ffi-0.29.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pyo3-ffi-0.29.2/LICENSE-MIT) |
| pyo3-macros | 0.29.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pyo3-macros-0.29.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pyo3-macros-0.29.2/LICENSE-MIT) |
| pyo3-macros-backend | 0.29.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/pyo3-macros-backend-0.29.2/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/pyo3-macros-backend-0.29.2/LICENSE-MIT) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/quote-1.0.47/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/quote-1.0.47/LICENSE-MIT) |
| radium | 0.7.0 | MIT | [LICENSE.txt](licenses/third-party/radium-0.7.0/LICENSE.txt) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [COPYRIGHT](licenses/third-party/rand_core-0.10.1/COPYRIGHT), [LICENSE-APACHE](licenses/third-party/rand_core-0.10.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/rand_core-0.10.1/LICENSE-MIT) |
| regex | 1.13.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/regex-1.13.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/regex-1.13.1/LICENSE-MIT) |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/regex-automata-0.4.18/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/regex-automata-0.4.18/LICENSE-MIT) |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/regex-syntax-0.8.11/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/regex-syntax-0.8.11/LICENSE-MIT) |
| ring | 0.17.14 | Apache-2.0 AND ISC | [LICENSE](licenses/third-party/ring-0.17.14/LICENSE), [LICENSE-BoringSSL](licenses/third-party/ring-0.17.14/LICENSE-BoringSSL), [LICENSE-other-bits](licenses/third-party/ring-0.17.14/LICENSE-other-bits) |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | [LICENSE-APACHE](licenses/third-party/rustls-0.23.46/LICENSE-APACHE), [LICENSE-ISC](licenses/third-party/rustls-0.23.46/LICENSE-ISC), [LICENSE-MIT](licenses/third-party/rustls-0.23.46/LICENSE-MIT) |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | [LICENSE-APACHE](licenses/third-party/rustls-native-certs-0.8.5/LICENSE-APACHE), [LICENSE](licenses/third-party/rustls-native-certs-0.8.5/LICENSE), [LICENSE-ISC](licenses/third-party/rustls-native-certs-0.8.5/LICENSE-ISC), [LICENSE-MIT](licenses/third-party/rustls-native-certs-0.8.5/LICENSE-MIT) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/rustls-pki-types-1.15.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/rustls-pki-types-1.15.1/LICENSE-MIT) |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/rustls-platform-verifier-0.7.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/rustls-platform-verifier-0.7.1/LICENSE-MIT) |
| rustls-webpki | 0.103.16 | ISC | [LICENSE](licenses/third-party/rustls-webpki-0.103.16/LICENSE) |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/rustversion-1.0.23/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/rustversion-1.0.23/LICENSE-MIT) |
| salsa20 | 0.11.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/salsa20-0.11.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/salsa20-0.11.0/LICENSE-MIT) |
| saturating | 0.1.0 | MIT | [LICENSE](licenses/third-party/saturating-0.1.0/LICENSE) |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/scopeguard-1.2.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/scopeguard-1.2.0/LICENSE-MIT) |
| scrypt | 0.12.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/scrypt-0.12.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/scrypt-0.12.0/LICENSE-MIT) |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/security-framework-3.7.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/security-framework-3.7.0/LICENSE-MIT) |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/security-framework-sys-2.17.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/security-framework-sys-2.17.0/LICENSE-MIT) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/serde-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/serde-1.0.229/LICENSE-MIT) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/serde_core-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/serde_core-1.0.229/LICENSE-MIT) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/serde_derive-1.0.229/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/serde_derive-1.0.229/LICENSE-MIT) |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/serde_json-1.0.154/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/serde_json-1.0.154/LICENSE-MIT) |
| sha1 | 0.11.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/sha1-0.11.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/sha1-0.11.0/LICENSE-MIT) |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/sha2-0.11.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/sha2-0.11.1/LICENSE-MIT) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/shlex-2.0.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/shlex-2.0.1/LICENSE-MIT) |
| simd-adler32 | 0.3.10 | MIT | [LICENSE.md](licenses/third-party/simd-adler32-0.3.10/LICENSE.md) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [LICENSE-Apache](licenses/third-party/simdutf8-0.1.5/LICENSE-Apache), [LICENSE-MIT](licenses/third-party/simdutf8-0.1.5/LICENSE-MIT) |
| smallvec | 1.16.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/smallvec-1.16.4/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/smallvec-1.16.4/LICENSE-MIT) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/socket2-0.6.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/socket2-0.6.5/LICENSE-MIT) |
| spki | 0.8.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/spki-0.8.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/spki-0.8.1/LICENSE-MIT) |
| subtle | 2.6.1 | BSD-3-Clause | [LICENSE](licenses/third-party/subtle-2.6.1/LICENSE) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/syn-2.0.119/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/syn-2.0.119/LICENSE-MIT) |
| syn | 3.0.9 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/syn-3.0.9/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/syn-3.0.9/LICENSE-MIT) |
| tap | 1.0.1 | MIT | [LICENSE.txt](licenses/third-party/tap-1.0.1/LICENSE.txt) |
| target-lexicon | 0.13.5 | Apache-2.0 WITH LLVM-exception | [LICENSE](licenses/third-party/target-lexicon-0.13.5/LICENSE) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/thiserror-2.0.21/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/thiserror-2.0.21/LICENSE-MIT) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/thiserror-impl-2.0.21/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/thiserror-impl-2.0.21/LICENSE-MIT) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [LICENSE](licenses/third-party/typenum-1.20.1/LICENSE), [LICENSE-APACHE](licenses/third-party/typenum-1.20.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/typenum-1.20.1/LICENSE-MIT) |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](licenses/third-party/unicode-ident-1.0.27/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/unicode-ident-1.0.27/LICENSE-MIT), [LICENSE-UNICODE](licenses/third-party/unicode-ident-1.0.27/LICENSE-UNICODE) |
| universal-hash | 0.6.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](licenses/third-party/universal-hash-0.6.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/universal-hash-0.6.1/LICENSE-MIT) |
| untrusted | 0.7.1 | ISC | [LICENSE.txt](licenses/third-party/untrusted-0.7.1/LICENSE.txt) |
| untrusted | 0.9.0 | ISC | [LICENSE.txt](licenses/third-party/untrusted-0.9.0/LICENSE.txt) |
| uuid | 1.29.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/uuid-1.29.0/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/uuid-1.29.0/LICENSE-MIT) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [LICENSE-APACHE](licenses/third-party/version_check-0.9.5/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/version_check-0.9.5/LICENSE-MIT) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [license-apache-2.0](licenses/third-party/windows-link-0.2.1/license-apache-2.0), [license-mit](licenses/third-party/windows-link-0.2.1/license-mit) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [license-apache-2.0](licenses/third-party/windows-sys-0.61.2/license-apache-2.0), [license-mit](licenses/third-party/windows-sys-0.61.2/license-mit) |
| wyz | 0.5.1 | MIT | [LICENSE.txt](licenses/third-party/wyz-0.5.1/LICENSE.txt) |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/zerocopy-0.8.63/LICENSE-APACHE), [LICENSE-BSD](licenses/third-party/zerocopy-0.8.63/LICENSE-BSD), [LICENSE-MIT](licenses/third-party/zerocopy-0.8.63/LICENSE-MIT) |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](licenses/third-party/zeroize-1.9.1/LICENSE-APACHE), [LICENSE-MIT](licenses/third-party/zeroize-1.9.1/LICENSE-MIT) |
| zmij | 1.0.23 | MIT | [LICENSE-MIT](licenses/third-party/zmij-1.0.23/LICENSE-MIT) |
