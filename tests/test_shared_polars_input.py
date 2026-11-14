"""Polars uses shared native facts, complete names, and actual base storage."""

# pyright: reportPrivateUsage=false
from typing import Any


from decimal import Decimal
import json
import subprocess
import sys
import pytest
from sqlaero import _sqlaero, json_dumps

pl = pytest.importorskip("polars")


@pytest.mark.parametrize("dtype,values,expected", [
    (pl.Int8, [-128, None, 127], [-128, None, 127]),
    (pl.UInt64, [0, None, 2**64 - 1], [0, None, 2**64 - 1]),
    (pl.Float32, [0.5, -0.0, float("nan")], [0.5, -0.0, None]),
    (pl.Float64, [0.5, float("inf"), -float("inf")], [0.5, None, None]),
    (pl.Boolean, [True, None, False], [True, None, False]),
    (pl.Null, [None, None], [None, None]),
    (pl.String, ["a\0x", None, "中"], ["a\0x", None, "中"]),
    (pl.Decimal(10, 3), [Decimal("1.250"), None], ["1.250", None]),
    (pl.Categorical, ["b", None, "a"], ["b", None, "a"]),
    (pl.Enum(["a", "b"]), ["b", None, "a"], ["b", None, "a"]),
    (pl.Date, [-1, None], ["1969-12-31", None]),
    (pl.Datetime("ns"), [-1, None], ["1969-12-31T23:59:59.999999999", None]),
    (pl.Datetime("ns", "Asia/Shanghai"), [-1, None], ["1970-01-01T07:59:59.999999999+08:00", None]),
    (pl.Duration("ns"), [-1234567891, None], ["-PT1.234567891S", None]),
    (pl.Time, [1, 86_399_999_999_999, None], ["00:00:00.000000001", "23:59:59.999999999", None]),
    (pl.List(pl.Int64), [[7, None], None, []], [[7, None], None, []]),
    (pl.Array(pl.Int64, 2), [[7, 8], None], [[7, 8], None]),
    (pl.Struct([pl.Field("id", pl.Int64), pl.Field("name", pl.String)]),
     [{"id": 7, "name": "ab"}, None, {"id": None, "name": None}],
     [{"id": 7, "name": "ab"}, None, {"id": None, "name": None}]),
])
def test_logical_values_enter_the_same_json_target(dtype: Any, values: Any, expected: Any) -> None:
    source = pl.Series("ignored", values, dtype=dtype)
    assert json.loads(json_dumps(source)) == expected


@pytest.mark.parametrize("kind", ["series", "frame"])
def test_supported_real_subclasses_ignore_replacement_storage_and_conversion_hooks(kind: str) -> None:
    base = pl.Series if kind == "series" else pl.DataFrame

    class Child(base):
        def clone(self):
            raise AssertionError("replacement clone")

        def __arrow_c_stream__(self, *args: object):
            raise AssertionError("replacement export")

        def __len__(self):
            raise AssertionError("replacement length")

        def __getattribute__(self, name: str) -> Any:
            if name in ("_s", "_df", "__dict__", "dtype", "dtypes", "name", "columns", "height", "width", "shape"):
                raise AssertionError("replacement attribute " + name)
            parent: Any = super()
            return parent.__getattribute__(name)

    source = Child("ignored", [7, 8]) if kind == "series" else Child({"id": [7, 8]})
    assert json_dumps(source) == (b"[7,8]" if kind == "series" else b'[{"id":7},{"id":8}]')
    assert _sqlaero._bind_sql("SELECT %s", source, many=True) == [b"\x03SELECT 7", b"\x03SELECT 8"]


@pytest.mark.parametrize("kind", ["frame", "object_frame", "struct", "list", "array"])
def test_unsafe_nul_name_export_is_rejected_before_entering_producer_callback(kind: str) -> None:
    # A faulty producer export can abort at its C callback. Keep this source
    # boundary counterexample in an isolated process so other cases finish.
    code = "from tests.test_shared_polars_input import complete_names_case; complete_names_case(" + repr(kind) + ")"
    result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=20)
    assert result.returncode == 0, result.stderr


def complete_names_case(kind: str) -> None:
    dtype = pl.Struct([pl.Field("a\0x", pl.Int64), pl.Field("a\0y", pl.Int64)])
    row = {"a\0x": 7, "a\0y": 8}
    if kind in ("frame", "object_frame"):
        source = pl.DataFrame([pl.Series("a\0x", [7]), pl.Series("a\0y", [8], dtype=pl.Object if kind == "object_frame" else pl.Int64)])
        expected = [row]
    else:
        dtype = dtype if kind == "struct" else pl.List(dtype) if kind == "list" else pl.Array(dtype, 1)
        source = pl.Series([row] if kind == "struct" else [[row]], dtype=dtype)
        expected = [row] if kind == "struct" else [[row]]
    # User-approved source limit: the current Polars exporter aborts on these
    # names. The table/Struct examples above describe actual source values;
    # they must be rejected, never truncated or turned into different values.
    for operation in (json_dumps, _sqlaero._escape_projection):
        with pytest.raises(TypeError, match="Polars.*contains NUL.*before export"):
            operation(source)
    assert (source.to_dicts() if kind in ("frame", "object_frame") else source.to_list()) == expected


@pytest.mark.parametrize("empty", [False, True])
def test_root_series_name_safety_precedes_native_export_even_without_rows(empty: bool) -> None:
    source = pl.Series("a\0x", [] if empty else [7], dtype=pl.Int64)
    for operation in (json_dumps, _sqlaero._escape_projection):
        with pytest.raises(TypeError, match="Series name contains NUL"):
            operation(source)


def test_parent_null_does_not_make_an_unsafe_declared_c_schema_exportable() -> None:
    source = pl.Series([None], dtype=pl.Struct([pl.Field("a\0x", pl.Int64)]))
    with pytest.raises(TypeError, match="schema field 0.*contains NUL"):
        json_dumps(source)


def test_object_only_sources_do_not_enter_the_unsafe_native_export() -> None:
    series = pl.Series("a\0x", [7, None], dtype=pl.Object)
    assert json_dumps(series) == b"[7,null]"
    frame = pl.DataFrame([series, pl.Series("a\0y", [8, None], dtype=pl.Object)])
    assert json_dumps(frame) == b'[{"a\\u0000x":7,"a\\u0000y":8},{"a\\u0000x":null,"a\\u0000y":null}]'
    assert json_dumps(frame.slice(0, 0)) == b"[]"
    assert _sqlaero._bind_sql("SELECT %s,%s", frame) == [b"\x03SELECT 7,8", b"\x03SELECT NULL,NULL"]


@pytest.mark.parametrize("kind", ["native", "object"])
def test_empty_rows_and_actual_zero_field_rows_keep_their_shape(kind: str) -> None:
    source = pl.DataFrame({"id": pl.Series([7, 8], dtype=pl.Object if kind == "object" else pl.Int64)})
    assert json_dumps(source.slice(0, 0)) == b"[]"
    assert json_dumps(source.drop("id")) == b"[{},{}]"


@pytest.mark.parametrize("dtype,values", [(pl.Binary, [b"a", None]), (pl.Binary, []), (pl.Binary, [None])])
def test_binary_declared_schema_is_mysql_only_even_without_a_valid_value(dtype: Any, values: Any) -> None:
    source = pl.Series(values, dtype=dtype)
    with pytest.raises(TypeError, match="[Bb]inary|bytes"):
        json_dumps(source)
    _sqlaero._encode_value(source)


def test_object_leaves_use_base_values_and_preserve_null_origin() -> None:
    class ChangedInt(int):
        def __int__(self):
            raise AssertionError("replacement integer")
    source = pl.Series([ChangedInt(7), None, {"id": 8}], dtype=pl.Object)
    assert json_dumps(source) == b'[7,null,{"id":8}]'


def test_provider_rejects_wide_time_at_construction_before_driver_consumption() -> None:
    with pytest.raises(pl.exceptions.InvalidOperationError, match="conversion.*failed"):
        pl.Series([-1, 90_000_000_000_000], dtype=pl.Time)


@pytest.mark.parametrize("kind", ["struct", "list"])
def test_selected_struct_and_list_rows_use_the_common_many_layout(kind: str) -> None:
    source = pl.Series([{"id": 7, "name": "ab"}, {"id": 8, "name": "cd"}]) if kind == "struct" else pl.Series([[7, 8], [9, 10]])
    expected = [b"\x03SELECT 7,'ab'", b"\x03SELECT 8,'cd'"] if kind == "struct" else [b"\x03SELECT 7,8", b"\x03SELECT 9,10"]
    assert _sqlaero._bind_sql("SELECT %s,%s", source, many=True) == expected


@pytest.mark.parametrize("size", [0, 2])
def test_unapproved_declared_schema_is_not_admitted_by_empty_or_null_values(size: int) -> None:
    source = pl.Series([None] * size, dtype=pl.Int128)
    with pytest.raises(TypeError, match="unsupported.*dtype"):
        json_dumps(source)
    with pytest.raises(TypeError, match="unsupported.*dtype"):
        _sqlaero._encode_value(source)
