"""Public compression configuration and actual owned-connection snapshots."""

from typing import Any

import pytest
import sqlaero
from sqlaero import SSL
from test_public_connection import ROOT, fixture_port, live, requires_mysql


def configured(**values: Any) -> Any:
    return getattr(sqlaero, "Compression")(**values)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize(("algorithm", "level"), [("zlib", None), ("zstd", 3), ("zstd", 10)])
def test_public_compression_reports_actual_server_facts_and_retains_closed_snapshot(
    label: str, algorithm: str, level: int | None,
) -> None:
    config = configured(algorithms=algorithm, zstd_compression_level=level or 3)
    connection = live(label, compression=config)
    try:
        with connection.cursor() as cursor:
            cursor.execute("SELECT 10 AS v UNION ALL SELECT 20")
            assert cursor.fetchone() == (10,)
            columns = cursor.columns
            snapshot = getattr(connection, "compression")
            assert type(snapshot) is type(config)
            assert snapshot is getattr(connection, "compression")
            assert snapshot.algorithms == (algorithm,)
            assert snapshot.zstd_compression_level == level
            assert cursor.columns is columns and cursor.fetchone() == (20,)
            cursor.execute("SHOW SESSION STATUS LIKE 'Compression_algorithm'")
            assert cursor.fetchone() == ("Compression_algorithm", algorithm)
            if level is not None:
                cursor.execute("SHOW SESSION STATUS LIKE 'Compression_level'")
                assert cursor.fetchone() == ("Compression_level", str(level))
            for owner, attribute, value in ((connection, "compression", None),
                                            (snapshot, "algorithms", ("uncompressed",)),
                                            (snapshot, "zstd_compression_level", 22)):
                with pytest.raises(AttributeError):
                    setattr(owner, attribute, value)
            payload = "中文😀'\\\x00" * 50_000
            cursor.execute("SELECT %s", payload)
            assert cursor.fetchone() == (payload,)
    finally:
        connection.close()
    assert getattr(connection, "compression") is snapshot
    assert config.algorithms == (algorithm,) and config.zstd_compression_level == level
    if algorithm == "zlib":
        assert "level" not in repr(snapshot)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_default_and_explicit_raw_permissions_return_none_without_mutating_the_config(label: str) -> None:
    raw = configured(algorithms=None, zstd_compression_level=10)
    for settings in ({}, {"compression": None}, {"compression": "uncompressed"}, {"compression": raw}):
        connection = live(label, **settings)
        assert getattr(connection, "compression") is None
        connection.close()
        assert getattr(connection, "compression") is None
    assert raw.algorithms == ("uncompressed",) and raw.zstd_compression_level is None


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_shorthand_permissions_and_one_reusable_config_match_native_negotiation(label: str) -> None:
    config = configured(algorithms=("zstd", "zlib", "uncompressed"), zstd_compression_level=10)
    with live(label, compression=config) as first, live(label, compression=config) as second:
        a, b = getattr(first, "compression"), getattr(second, "compression")
        assert a is not b and a.algorithms == b.algorithms == ("zlib",)
        assert a.zstd_compression_level is b.zstd_compression_level is None
        assert config.algorithms == ("zlib", "zstd", "uncompressed")
        assert config.zstd_compression_level == 10
    for source in (" ZSTD,uncompressed ", ["ZSTD", "uncompressed"], ("zstd", "uncompressed")):
        with live(label, compression=source) as connection:
            snapshot = getattr(connection, "compression")
            assert snapshot.algorithms == ("zstd",) and snapshot.zstd_compression_level == 3


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_public_zstd_and_verified_tls_share_the_same_transport_and_snapshot(label: str) -> None:
    with live(label, host="127.0.0.1", port=fixture_port(label), unix_socket=None,
              ssl=SSL(mode="VERIFY_IDENTITY", ca_file=ROOT / "tls/ca.pem"),
              compression="zstd") as connection, connection.cursor() as cursor:
        assert getattr(connection, "tls_info") is not None
        assert getattr(connection, "compression").algorithms == ("zstd",)
        cursor.execute("SELECT %s", "TLS+zstd 中文😀")
        assert cursor.fetchone() == ("TLS+zstd 中文😀",)
