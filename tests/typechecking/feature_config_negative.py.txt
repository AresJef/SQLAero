"""Removed names and invalid public types remain static errors."""

from sqlaero import Compression, Connection, SSL, TLSCertificate, TLSInfo, connect


def rejected(connection: Connection, info: TLSInfo, certificate: TLSCertificate) -> None:
    connection.ssl_mode  # error: removed Connection alias
    connection.tls_version  # error: removed Connection alias
    connection.compression = None  # error: read only
    connection.local_infile = True  # error: read only
    connection.multi_statements = False  # error: explicit remote setter
    connection.set_multi_statements(1)  # error: bool required
    connection.create_savepoint(None)  # error: explicit str
    connection.net_read_timeout = 1  # error: explicit remote setter
    connection.set_wait_timeout(0.5)  # error: server seconds stay int
    connect(user="check only", server_public_key_path="old")  # error: removed keyword
    connect(user="check only", init_sql=[1])  # error: finite SQL strings
    connect(user="check only", local_infile=1)  # error: bool required
    SSL(ca_file=b"path")  # error: top-level bytes are not PathInput
    SSL(ca_data=bytearray(b"PEM"))  # error: no buffer coercion
    SSL(cipher="legacy")  # error: no public cipher configuration
    SSL(append_system_ca=True)  # error: no parallel CA alias
    SSL().max_tls_version = None  # error: read only
    Compression()  # error: algorithms required
    Compression(algorithms="zstd", zstd_compression_level=3.0)  # error: level int
    config = Compression(algorithms="zlib")
    config.algorithms = ("zstd",)  # error: read only
    config.zstd_compression_level = 10  # error: read only
    info.version = "TLSv1.3"  # error: read only
    certificate.subject = "changed"  # error: read only
    TLSInfo()  # error: factory only
    TLSCertificate()  # error: factory only
