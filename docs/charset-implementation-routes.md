# MySQL character-set implementation routes

This is the implementation routing decision for P01.2, not a claim that all
codecs have already been implemented or validated. D05 and D07–D09 remain the
public contract. Product conversion never calls a Python codec or OS iconv.

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
