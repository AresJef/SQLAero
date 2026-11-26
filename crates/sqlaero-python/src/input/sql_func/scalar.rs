//! Scalar control, binary, network, and session functions.

use super::calls::{fixed_calls, optional_calls, variadic_calls};
use super::{Kind, SqlFunction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::Form;

fixed_calls! {
    Choose, "IF", (condition, if_true, if_false), "Select a value according to the SQL condition.";
    IfNull, "IFNULL", (value, fallback), "Return fallback when value is SQL NULL.";
    IsNull, "ISNULL", (value), "Test whether value is SQL NULL.";
    NullIf, "NULLIF", (left, right), "Return SQL NULL when left equals right, otherwise left.";
    Charset, "CHARSET", (value), "Return the character set of value.";
    Coercibility, "COERCIBILITY", (value), "Return the collation coercibility of value.";
    Collation, "COLLATION", (value), "Return the collation of value.";
    Compress, "COMPRESS", (value), "Compress value into MySQL's binary format.";
    ConnectionId, "CONNECTION_ID", (), "Return the current server connection identifier.";
    CurrentRole, "CURRENT_ROLE", (), "Return the active roles of the current session.";
    CurrentUser, "CURRENT_USER", (), "Return the MySQL account used to check privileges.";
    Database, "DATABASE", (), "Return the current default database." => [Schema, "SCHEMA"];
    IcuVersion, "ICU_VERSION", (), "Return the ICU library version used by the server.";
    InetAton, "INET_ATON", (address), "Convert a text IPv4 address to an integer.";
    InetNtoa, "INET_NTOA", (address), "Convert an integer IPv4 address to text.";
    Inet6Aton, "INET6_ATON", (address), "Convert a text IPv4 or IPv6 address to binary.";
    Inet6Ntoa, "INET6_NTOA", (address), "Convert a binary IPv4 or IPv6 address to text.";
    IsIpv4, "IS_IPV4", (address), "Test whether address is valid IPv4 text.";
    IsIpv4Compat, "IS_IPV4_COMPAT", (address), "Test whether a binary address is IPv4-compatible IPv6.";
    IsIpv4Mapped, "IS_IPV4_MAPPED", (address), "Test whether a binary address is IPv4-mapped IPv6.";
    IsIpv6, "IS_IPV6", (address), "Test whether address is valid IPv6 text.";
    IsUuid, "IS_UUID", (value), "Test whether value is a valid MySQL UUID string.";
    RandomBytes, "RANDOM_BYTES", (length), "Return length random bytes from the server.";
    RowCount, "ROW_COUNT", (), "Return the affected row count from the preceding statement.";
    Sha2, "SHA2", (value, bits), "Return a hexadecimal SHA-2 digest using the specified digest width.";
    Uncompress, "UNCOMPRESS", (value), "Decode MySQL compressed data to its original bytes.";
    UncompressedLength, "UNCOMPRESSED_LENGTH", (value), "Return the original byte length of MySQL compressed data.";
    User, "USER", (), "Return the supplied client username and client host." => [SessionUser, "SESSION_USER", SystemUser, "SYSTEM_USER"];
    Uuid, "UUID", (), "Generate a version-1 UUID string when the server evaluates the expression.";
    UuidShort, "UUID_SHORT", (), "Generate a server-based unsigned integer identifier.";
    Version, "VERSION", (), "Return the server's version string.";
}

optional_calls! {
    AesDecrypt, "AES_DECRYPT", (value, key), (iv, kdf, salt, info_or_iterations),
        "(value, key, iv=..., kdf=..., salt=..., info_or_iterations=...)",
        "Decrypt AES ciphertext using the session's block mode and optional key derivation.";
    AesEncrypt, "AES_ENCRYPT", (value, key), (iv, kdf, salt, info_or_iterations),
        "(value, key, iv=..., kdf=..., salt=..., info_or_iterations=...)",
        "Encrypt data using the session's block mode and optional key derivation.";
    BinToUuid, "BIN_TO_UUID", (value), (swap_flag), "(value, swap_flag=...)",
        "Convert a binary UUID to text, optionally reversing the time-field swap.";
    LastInsertId, "LAST_INSERT_ID", (), (value), "(value=...)",
        "Read the last insert ID or set it for the session when value is supplied.";
    UuidToBin, "UUID_TO_BIN", (value), (swap_flag), "(value, swap_flag=...)",
        "Convert a UUID to binary, optionally swapping the time fields.";
}

variadic_calls! {
    Coalesce, "COALESCE", (value), values, "Return the first value that is not SQL NULL.";
    Greatest, "GREATEST", (first, second), values, "Return the greatest value according to MySQL comparison rules.";
    NumericInterval, "INTERVAL", (value, first), boundaries, "Return the position of value among increasing numeric boundaries.";
    Least, "LEAST", (first, second), values, "Return the least value according to MySQL comparison rules.";
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    register_variadic(module)
}
