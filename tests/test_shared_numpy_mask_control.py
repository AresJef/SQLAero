"""Mask identity and owner checks share the existing operation boundaries."""

from datetime import datetime, timedelta, tzinfo

import pytest

from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, _sqlaero, json_dumps
from test_public_connection import live, requires_mysql

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("change", ["shape", "dtype", "missing", "object"])
def test_invalid_actual_mask_storage_is_rejected(change: str) -> None:
    value = np.ma.array([7, 8], mask=[False, True])
    if change == "shape":
        value.__dict__["_mask"] = np.array([False])
    elif change == "dtype":
        value.__dict__["_mask"] = np.array([0, 1], dtype="i8")
    elif change == "missing":
        del value.__dict__["_mask"]
    else:
        value.__dict__["_mask"] = object()
    with pytest.raises(TypeError, match="mask"):
        json_dumps(value)


@pytest.mark.parametrize("target", ["json", "mysql"])
@pytest.mark.parametrize("replace_dictionary", [False, True])
@pytest.mark.parametrize("raise_original", [False, True])
def test_mask_replacement_after_payload_callback_is_not_reused(
    target: str, replace_dictionary: bool, raise_original: bool,
) -> None:
    original = MemoryError("original mask source protocol")
    source = np.ma.array([None, object()], dtype=object, mask=[False, True])

    class Zone(tzinfo):
        def utcoffset(self, value: datetime | None) -> timedelta:
            if replace_dictionary:
                namespace = dict(source.__dict__)
                namespace["_mask"] = np.array([False, False])
                source.__dict__ = namespace
            else:
                source.mask = [False, False]
            if raise_original:
                raise original
            return timedelta(0)

    source.data[0] = datetime(2026, 1, 2, tzinfo=Zone())
    with pytest.raises(MemoryError if raise_original else BufferError) as caught:
        if target == "json":
            json_dumps(source)
        else:
            _sqlaero._encode_value(source)
    if raise_original:
        assert caught.value is original
    else:
        assert "mask changed" in str(caught.value)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_actual_database_stores_masked_values_as_null(label: str, kind: type, prepared: bool) -> None:
    with live(label) as connection:
        with connection.cursor(kind, prepared=prepared) as current:
            current.execute("CREATE TEMPORARY TABLE mask_facts (id INT AUTO_INCREMENT PRIMARY KEY, value INT)")
            value = np.ma.array([7, 999, 8], mask=[False, True, False])
            current.execute("INSERT INTO mask_facts(value) VALUES (%s)", value, many=True)
            current.execute("INSERT INTO mask_facts(value) VALUES (%s)", np.ma.array(123, mask=True))
        with connection.cursor() as check:
            check.execute("SELECT value FROM mask_facts ORDER BY id")
            assert check.fetchall() == ((7,), (None,), (8,), (None,))
