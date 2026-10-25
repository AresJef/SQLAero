# MySQL character-set implementation routes

This records the P01.2 routing decision and its implemented P03.2 strict
conversion paths. D05 and D07–D09 remain the public contract. Product conversion
never calls a Python codec or OS iconv. Surrogateescape output is completed at
the P04 Python materialization boundary; it is not claimed complete here.

Both isolated servers were queried on 2026-10-26: MySQL 8.4.13 and 9.7.4 return
the same 41 character-set families and 286 collations. Excluding utf8mb3, ucs2,
utf16, utf16le and utf32 leaves **36 client families and 177 collations**.
The raw catalogs and hashes are in `evidence/rust-sync-kernel/startup/`.
The handshake uses its separately fixed utf8mb4 context; collation IDs above
255 are applied after authentication, never truncated to one byte.

## Static routing

`SingleByte` means one generated MySQL byte-to-Unicode table and its canonical
reverse map, used by the same strict encoder/decoder. Tables are generated from
observable SQL conversions on the two isolated servers, not copied from MySQL
GPL source. Undefined sequences must be identified from conversion diagnostics;
a replacement question mark is not accepted as a valid mapping. Independent
standard mapping/PSF codec checks are test oracles, never product fallbacks.

`Multibyte` means the named fixed encoding_rs implementation plus a static
MySQL-specific validity/mapping delta in that codec. The delta can be empty only
after verification. GB2312 must not accept the whole GBK repertoire; Windows or
WHATWG extensions must not silently expand the stricter MySQL families. This
is one deterministic route, not “try another encoding on error”.

| MySQL family | Product route | Independent verification reference |
| --- | --- | --- |
| utf8mb4 | Native UTF-8 | Unicode scalar boundaries; server round trips |
| ascii | Native 7-bit ASCII | All 256 input bytes and U+007F/U+0080 boundary |
| binary | Native byte-valued mapping | All 256 bytes; binary/text field metadata |
| armscii8 | SingleByte | ARMSCII-8 standard mapping and both servers |
| cp1250 | SingleByte | PSF cp1250 mapping |
| cp1251 | SingleByte | PSF cp1251 mapping |
| cp1256 | SingleByte | PSF cp1256 mapping |
| cp1257 | SingleByte | PSF cp1257 mapping |
| cp850 | SingleByte | PSF cp850 mapping |
| cp852 | SingleByte | PSF cp852 mapping |
| cp866 | SingleByte | PSF cp866 mapping |
| dec8 | SingleByte | DEC multinational mapping and both servers |
| geostd8 | SingleByte | Georgian standard mapping and both servers |
| greek | SingleByte | PSF iso8859_7; explicit MySQL deltas |
| hebrew | SingleByte | PSF iso8859_8; explicit MySQL deltas |
| hp8 | SingleByte | HP Roman-8 mapping and both servers |
| keybcs2 | SingleByte | Kamenicky mapping and both servers |
| koi8r | SingleByte | PSF koi8_r mapping |
| koi8u | SingleByte | PSF koi8_u mapping |
| latin1 | SingleByte | Windows-1252/MySQL distinctions, especially 0x80–0x9F |
| latin2 | SingleByte | PSF iso8859_2 mapping |
| latin5 | SingleByte | PSF iso8859_9 mapping |
| latin7 | SingleByte | PSF iso8859_13 mapping |
| macce | SingleByte | PSF mac_latin2 mapping |
| macroman | SingleByte | PSF mac_roman mapping |
| swe7 | SingleByte | Swedish 7-bit national replacement positions |
| tis620 | SingleByte | PSF tis_620; reject Windows-874 repertoire expansion |
| big5 | Multibyte: encoding_rs::BIG5 + MySQL delta | PSF big5; HKSCS/WHATWG extension differences |
| cp932 | Multibyte: encoding_rs::SHIFT_JIS + MySQL delta | PSF cp932; canonical duplicate mappings |
| sjis | Multibyte: encoding_rs::SHIFT_JIS + MySQL validity/delta | PSF shift_jis; CP932 extension exclusion |
| euckr | Multibyte: encoding_rs::EUC_KR + MySQL delta | PSF euc_kr; Windows-949 extension differences |
| gb2312 | Multibyte: encoding_rs::GBK + GB2312 validity/delta | PSF gb2312; GBK extension exclusion |
| gbk | Multibyte: encoding_rs::GBK + MySQL delta | PSF gbk; euro/duplicate mapping boundaries |
| gb18030 | Multibyte: encoding_rs::GB18030 + MySQL delta | PSF gb18030; two-/four-byte range and revision boundaries |
| ujis | Multibyte: encoding_rs::EUC_JP + MySQL delta | PSF euc_jp; JIS X 0201/0208/0212 |
| eucjpms | Multibyte: encoding_rs::EUC_JP + MySQL extension table | Microsoft Japanese extensions; both servers |

The static charset/collation catalog is generated from server metadata and
contains factual identifiers/defaults. UTF-8 and UTF8 aliases resolve to
utf8mb4; utf8mb3 and its collations remain rejected. A charset alone uses its
catalog default; a collation alone determines its charset; a supplied pair
must match. User text is never interpolated as an unchecked SQL identifier.

## Data provenance and validation boundary

The implementation uses the MIT SQLAero generators and observed SQL conversion
results, together with the already selected encoding_rs 0.8.42 package and its
MIT/Apache-2.0 licensing. No MySQL source table or algorithm is copied. Any
external standard mapping data actually incorporated later must retain its
specific provenance/license; a name in this routing table is not permission
to copy an arbitrary third-party mapping file.

P03/P04 must verify both directions and canonical reverse choices. Single-byte
families cover every byte. Multibyte families cover valid/invalid lead/trail
ranges, complete two-byte repertoires where applicable, meaningful three-/
four-byte boundaries, partial sequences and SQL metacharacter trail bytes.
Surrogateescape output is distinct from strict input encoding. Server
observations, independent expected data and the Rust implementation must not
all be generated by one unchecked transformation.

## P03.2 verification

Both server lines agree on all 6,144 single-byte observations and 354,407
multibyte candidates per line. Native conversion was checked against these
separately saved values and canonical reverse choices. The PSF codecs provide
an independent oracle; explicit server differences (cp1256, cp866 and koi8u)
are recorded in tests/test_text_input.py, rather than concealed as aliases.
Undefined legacy U+FFFD slots are rejected even when the server emits no
warning. An actual U+FFFD remains a valid GB18030/UTF-8 Unicode character.

MySQL [8.4](https://dev.mysql.com/doc/refman/8.4/en/charset-gb18030.html) and
[9.7](https://dev.mysql.com/doc/refman/9.7/en/charset-gb18030.html) specify
GB18030-2005; the current [WHATWG standard](https://encoding.spec.whatwg.org/)
contains later revisions. The fixed component is therefore used with measured
MySQL deltas. CPython's reference codec retains the older U+1E3F/U+E7C7 mapping;
the independent full Unicode scalar-space test accounts explicitly for that
two-point 2005 swap, not by reading the product table. Width follows the full
GB18030 grammar, not just the lead bytes present in boundary samples.

Real native queries passed every client family on MySQL 8.4.13 and 9.7.4 under
both backslash modes, including NUL, quote/backslash-sensitive characters,
Unicode/multibyte values and binary CAST numeric/string semantics. `binary`
uses its actual byte-valued semantics; it is not treated as Unicode transcoding.
Evidence: startup/codec-real.log, codec-matrix.log, codec-single-matrix.log,
text-python-all.log, text-pyright.log and codec-clippy.log.

Generation is maintainer-only: tools/charset_tables.py observes the private
servers, and the core's charset_delta example compares those observations with
the fixed encoding_rs public API. Generated Rust data ships with the product;
normal build/import/conversion never starts MySQL or runs a generator. No
server source, private encoding_rs table or Python runtime codec is embedded.
