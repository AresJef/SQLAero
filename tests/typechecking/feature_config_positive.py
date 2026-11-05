"""Configuration and local helpers follow their final public type contracts."""

from datetime import datetime
from os import PathLike
from typing import Literal, assert_type

from sqlaero import Compression, Connection, SSL, TLSCertificate, TLSInfo, connect


def configured(path: PathLike[bytes], connection: Connection) -> None:
    tls = SSL(min_tls_version=" TLS 1.2 ", max_tls_version="1.3", ca_dir=path,
              ca_mode="APPEND", cert_data="certificate PEM", key_data=b"key PEM")
    assert_type(tls.min_tls_version, Literal["TLSv1.2", "TLSv1.3"])
    assert_type(tls.max_tls_version, Literal["TLSv1.2", "TLSv1.3"] | None)
    config = Compression(algorithms=["zstd", "uncompressed"], zstd_compression_level=10)
    assert_type(config.algorithms, tuple[str, ...])
    assert_type(config.zstd_compression_level, int | None)
    assert_type(connection.compression, Compression | None)
    assert_type(connection.tls_info, TLSInfo | None)
    assert_type(connection.local_infile, bool)
    assert_type(connection.multi_statements, bool)
    assert_type(connection.set_multi_statements(False), None)
    assert_type(connection.create_savepoint("step"), None)
    assert_type(connection.rollback_to_savepoint(name="step"), None)
    assert_type(connection.release_savepoint("step"), None)
    assert_type(connection.show_warnings(), tuple[tuple[str, int, str], ...])
    assert_type(connection.escape([1, 2]), str | tuple[str, ...] | list[str | tuple[str, ...]])
    assert_type(connection.connect_timeout, int | float | None)
    connection.operation_timeout = 0.000_001
    connection.read_timeout = None
    connection.write_timeout = 0.25
    assert_type(connect(user="check only", server_public_key=path, compression=config,
                        init_sql=("SET @v=1", "SELECT @v"), local_infile=True,
                        multi_statements=False, connect_timeout=0.5), Connection)
    assert_type(connect(user="check only", server_public_key=b"SPKI PEM", compression="zstd"), Connection)


def diagnostics(info: TLSInfo, certificate: TLSCertificate) -> None:
    assert_type(info.mode, Literal["REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"])
    assert_type(info.version, Literal["TLSv1.2", "TLSv1.3"])
    assert_type(info.peer_certificates, tuple[TLSCertificate, ...])
    assert_type(info.session_reused, bool)
    assert_type(certificate.serial_number, int)
    assert_type(certificate.not_after, datetime)
    assert_type(certificate.dns_names, tuple[str, ...])
    assert_type(certificate.to_der(), bytes)
    assert_type(certificate.to_pem(), str)
