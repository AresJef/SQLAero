"""Polars uses public Arrow exports and an explicit Python Object boundary."""

# pyright: reportPrivateUsage=false

from __future__ import annotations

from collections.abc import Callable
from decimal import Decimal
import ctypes
import math
import subprocess
import sys
from typing import Any, cast

import pytest

from sqlaero import _sqlaero

pl = pytest.importorskip("polars")


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(pl.Series([1, None, -3], dtype=pl.Int64), b"(1,NULL,-3)", id="int64"),
        pytest.param(pl.Series([0, 2**64 - 1], dtype=pl.UInt64), b"(0,18446744073709551615)", id="uint64"),
        pytest.param(pl.Series([True, None, False]), b"(1,NULL,0)", id="bool"),
        pytest.param(pl.Series([None, None], dtype=pl.Null), b"(NULL,NULL)", id="null"),
        pytest.param(pl.Series(["a", None, "中"]), "('a',NULL,'中')".encode(), id="string"),
        pytest.param(pl.Series([b"A\0", None]), b"(CAST(X'4100' AS BINARY),NULL)", id="binary"),
        pytest.param(pl.Series([Decimal("1.25"), None], dtype=pl.Decimal(10, 2)), b"(1.25,NULL)", id="decimal"),
    ],
)
def test_polars_native_series_reuses_value_writer(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


@pytest.mark.parametrize(
    "dtype,low,high",
    [
        (pl.Int8, -(2**7), 2**7 - 1), (pl.Int16, -(2**15), 2**15 - 1),
        (pl.Int32, -(2**31), 2**31 - 1), (pl.Int64, -(2**63), 2**63 - 1),
        (pl.UInt8, 0, 2**8 - 1), (pl.UInt16, 0, 2**16 - 1),
        (pl.UInt32, 0, 2**32 - 1), (pl.UInt64, 0, 2**64 - 1),
    ],
)
def test_polars_all_native_integer_widths_keep_signedness(dtype: object, low: int, high: int) -> None:
    value = pl.Series([low, None, high], dtype=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == f"({low},NULL,{high})".encode()


@pytest.mark.parametrize("dtype", [pl.Float32, pl.Float64])
def test_polars_native_float_value_negative_zero_and_nonfinite_policy(dtype: object) -> None:
    value = pl.Series([0.1, -0.0, float("nan"), float("inf"), -float("inf")], dtype=dtype)
    parts = _sqlaero._encode_value(value).to_bytes()[1:-1].split(b",")
    assert float(parts[0]) == value.item(0)
    assert float(parts[1]) == 0.0 and math.copysign(1.0, float(parts[1])) < 0
    assert parts[2:] == [b"NULL", b"NULL", b"NULL"]


@pytest.mark.parametrize("unit,fraction", [("ms", ".999"), ("us", ".999999"), ("ns", ".999999999")])
@pytest.mark.parametrize("zone", [None, "Asia/Shanghai"])
def test_polars_datetime_keeps_epoch_value_in_utc(unit: str, fraction: str, zone: str | None) -> None:
    value = pl.Series([-1, None], dtype=pl.Datetime(unit, zone))
    assert _sqlaero._encode_value(value).to_bytes() == f"('1969-12-31 23:59:59{fraction}',NULL)".encode()


@pytest.mark.parametrize("unit,fraction", [("ms", ".001"), ("us", ".000001"), ("ns", ".000000001")])
def test_polars_duration_keeps_signed_source_precision(unit: str, fraction: str) -> None:
    value = pl.Series([-1, None], dtype=pl.Duration(unit))
    assert _sqlaero._encode_value(value).to_bytes() == f"('-00:00:00{fraction}',NULL)".encode()


def test_polars_date_and_clock_are_not_duration_values() -> None:
    assert _sqlaero._encode_value(pl.Series([-1, None], dtype=pl.Date)).to_bytes() == b"('1969-12-31',NULL)"
    assert _sqlaero._encode_value(pl.Series([1, None], dtype=pl.Time)).to_bytes() == b"('00:00:00.000000001',NULL)"


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_polars_series_keeps_explicit_one_dimensional_roles(many: bool, itemize: bool) -> None:
    value = pl.Series([1, None, 3])
    if many:
        sql = "SELECT %s"
        expected = [b"\x03SELECT 1", b"\x03SELECT NULL", b"\x03SELECT 3"]
    elif itemize:
        sql = "SELECT %s,%s,%s"
        expected = [b"\x03SELECT 1,NULL,3"]
    else:
        sql = "SELECT %s"
        expected = [b"\x03SELECT (1,NULL,3)"]
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_polars_frame_is_a_natural_row_source(many: bool, itemize: bool) -> None:
    value = pl.DataFrame({"x": [1, 2], "y": ["a", "b"]})
    sql = "SELECT %s,%s" if itemize else "SELECT %s"
    expected = (
        [b"\x03SELECT 1,'a'", b"\x03SELECT 2,'b'"]
        if itemize
        else [b"\x03SELECT (1,'a')", b"\x03SELECT (2,'b')"]
    )
    assert _sqlaero._bind_sql(sql, value, many=many, itemize=itemize) == expected


@pytest.mark.parametrize("kind", ["series", "frame", "zero-columns"])
@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
def test_polars_empty_and_zero_column_shapes_keep_every_logical_row(kind: str, many: bool, itemize: bool) -> None:
    if kind == "series":
        value = pl.Series([], dtype=pl.Int64)
    elif kind == "frame":
        value = pl.DataFrame({"x": pl.Series([], dtype=pl.Int64)})
    else:
        # Polars drop preserves height; selecting no expressions would instead
        # create a 0x0 frame and would not verify a nonempty zero-column source.
        value = pl.DataFrame({"x": [1, 2, 3]}).drop("x")
        assert value.shape == (3, 0)
    count = 3 if kind == "zero-columns" else 1
    assert _sqlaero._bind_sql("SELECT 1", value, many=many, itemize=itemize) == [b"\x03SELECT 1"] * count
    with pytest.raises(_sqlaero.ProgrammingError):
        _sqlaero._bind_sql("SELECT %s", value, many=many, itemize=itemize)


@pytest.mark.parametrize("dtype", [pl.Categorical, pl.Enum(["a", "b"])])
def test_polars_classification_uses_labels_not_physical_codes(dtype: object) -> None:
    value = pl.Series(["b", None, "a", "b"], dtype=dtype)
    assert _sqlaero._encode_value(value).to_bytes() == b"('b',NULL,'a','b')"
    assert _sqlaero._bind_sql("SELECT %s", value.to_frame()) == [
        b"\x03SELECT 'b'", b"\x03SELECT NULL", b"\x03SELECT 'a'", b"\x03SELECT 'b'"
    ]


@pytest.mark.parametrize("dtype", [pl.Categorical, pl.Enum(["a'\\", "b中"])])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_polars_repeated_category_columns_keep_independent_labels_and_quote_modes(dtype: object, no_backslash: bool) -> None:
    left = ["a'\\", "a'\\", None, "b中", "b中", "a'\\"] * 6
    right = list(reversed(left))
    value = pl.DataFrame([
        pl.Series("left", left, dtype=dtype), pl.Series("right", right, dtype=dtype),
    ])
    expected = _sqlaero._bind_sql("SELECT %s,%s", list(zip(left, right, strict=True)), many=True, no_backslash=no_backslash)
    assert _sqlaero._bind_sql("SELECT %s,%s", value, no_backslash=no_backslash) == expected


def test_polars_category_reuse_never_caches_object_conversion_hooks() -> None:
    calls: list[int] = []

    class CountingInt(int):
        def __int__(self) -> int:
            calls.append(1)
            return len(calls)

    source = CountingInt(7)
    value = pl.DataFrame([
        pl.Series("category", ["a'"] * 32, dtype=pl.Categorical),
        pl.Series("objects", [source] * 32, dtype=pl.Object),
    ])
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [b"\x03SELECT 'a\\''," + str(index).encode() for index in range(1, 33)]
    assert calls == [1] * 32


@pytest.mark.parametrize("length", [1, 100, 1000])
@pytest.mark.parametrize("high_cardinality", [False, True], ids=["low", "high"])
def test_polars_small_and_large_category_sources_keep_actual_labels(length: int, high_cardinality: bool) -> None:
    labels = [f"label{i}'" for i in range(length if high_cardinality else 3)]
    values = [labels[index % len(labels)] for index in range(length)]
    value = pl.Series(values, dtype=pl.Categorical)
    assert _sqlaero._encode_value(value).to_bytes() == _sqlaero._encode_value(values).to_bytes()


@pytest.mark.parametrize("dtype", [pl.Categorical, pl.Enum(["a", "b"])])
@pytest.mark.parametrize("kind", ["list", "struct"])
def test_polars_nested_classification_keeps_labels_and_parent_validity(dtype: object, kind: str) -> None:
    if kind == "list":
        value = pl.Series([["b", None, "a"], None, []], dtype=pl.List(dtype))
        expected = [b"\x03SELECT ('b',NULL,'a')", b"\x03SELECT NULL", b"\x03SELECT ()"]
    else:
        value = pl.Series([{"label": "b"}, None, {"label": None}], dtype=pl.Struct([pl.Field("label", dtype)]))
        expected = [b"\x03SELECT ('b')", b"\x03SELECT NULL", b"\x03SELECT (NULL)"]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == expected


def test_polars_nested_columns_keep_one_cell_and_struct_schema_order() -> None:
    lists = pl.Series("lists", [[1, None], None, []], dtype=pl.List(pl.Int64))
    arrays = pl.Series("arrays", [[1, 2], None, [3, 4]], dtype=pl.Array(pl.Int64, 2))
    structures = pl.Series(
        "structs", [{"second": 2, "first": 1}, None, {"first": 3, "second": 4}],
        dtype=pl.Struct([pl.Field("first", pl.Int64), pl.Field("second", pl.Int64)]),
    )
    assert _sqlaero._bind_sql("SELECT %s,%s,%s", pl.DataFrame([lists, arrays, structures])) == [
        b"\x03SELECT (1,NULL),(1,2),(1,2)",
        b"\x03SELECT NULL,NULL,NULL",
        b"\x03SELECT (),(3,4),(3,4)",
    ]


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        pytest.param(pl.Series([None, None], dtype=pl.Null), b"(NULL,NULL)", id="root-null"),
        pytest.param(pl.Series([[None], None, []], dtype=pl.List(pl.Null)), b"((NULL),NULL,())", id="list-null"),
        pytest.param(pl.Series([[None, None], None], dtype=pl.Array(pl.Null, 2)), b"((NULL,NULL),NULL)", id="fixed-array-null"),
        pytest.param(
            pl.Series([{"a": None, "b": 2}, None], dtype=pl.Struct([pl.Field("a", pl.Null), pl.Field("b", pl.Int64)])),
            b"((NULL,2),NULL)",
            id="struct-null",
        ),
    ],
)
def test_polars_null_storage_is_supported_at_every_native_nesting_level(value: object, expected: bytes) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == expected


def test_polars_null_column_uses_the_same_import_boundary_as_nested_null() -> None:
    value = pl.DataFrame({"nulls": pl.Series([None, None], dtype=pl.Null), "ints": [1, 2]})
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [b"\x03SELECT NULL,1", b"\x03SELECT NULL,2"]


@pytest.mark.parametrize(
    "kind,fail",
    [(kind, fail) for kind in ("root", "table", "nested") for fail in (False, True)] + [("import", True)],
)
def test_polars_null_c_import_restores_headers_and_releases_exactly_once(
    monkeypatch: pytest.MonkeyPatch, kind: str, fail: bool
) -> None:
    """Observe real public exports; callbacks do not invent buffer addresses."""
    class Array(ctypes.Structure):
        pass

    array_pointer = ctypes.POINTER(Array)
    Array._fields_ = [
        ("length", ctypes.c_int64), ("null_count", ctypes.c_int64), ("offset", ctypes.c_int64),
        ("n_buffers", ctypes.c_int64), ("n_children", ctypes.c_int64),
        ("buffers", ctypes.POINTER(ctypes.c_void_p)), ("children", ctypes.POINTER(array_pointer)),
        ("dictionary", array_pointer), ("release", ctypes.c_void_p), ("private_data", ctypes.c_void_p),
    ]

    class Stream(ctypes.Structure):
        _fields_ = [(name, ctypes.c_void_p) for name in ("get_schema", "get_next", "get_last_error", "release", "private_data")]

    stream_pointer = ctypes.POINTER(Stream)
    get_next_type = ctypes.PYFUNCTYPE(ctypes.c_int, stream_pointer, array_pointer)
    release_type = ctypes.PYFUNCTYPE(None, array_pointer)
    get_pointer = ctypes.pythonapi.PyCapsule_GetPointer
    get_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
    get_pointer.restype = ctypes.c_void_p
    callbacks: list[object] = []
    releases: list[int] = []
    failures: list[str] = []

    def snapshot_tree(node: Array) -> tuple[bytes, list[object]]:
        return bytes(node), [snapshot_tree(node.children[i].contents) for i in range(node.n_children)]

    def wrap_export(original: Callable[..., object]) -> Callable[..., object]:
        def export(source: object, requested_schema: object = None) -> object:
            capsule = original(source, requested_schema)
            stream = ctypes.cast(get_pointer(capsule, b"arrow_array_stream"), stream_pointer)
            original_next = get_next_type(stream.contents.get_next)

            def next_array(raw_stream: object, raw_array: object) -> int:
                # ctypes creates pointer classes at runtime. This external C
                # boundary has the exact get_next_type prototype declared
                # above; keep the dynamic cast at its two incoming pointers.
                live_stream = cast(Any, raw_stream)
                result = cast(Any, raw_array)
                status = original_next(live_stream, result)
                if status == 0 and result.contents.release:
                    original_release = release_type(result.contents.release)
                    expected: tuple[bytes, list[object]]
                    restore_buffer_count: tuple[Any, int] | None = None
                    if kind == "import":
                        # Keep every actual pointer/allocated buffer intact,
                        # but omit the Boolean value-buffer declaration. The
                        # mature cheap validator must reject this safely after
                        # the neighboring Null header has been normalized.
                        malformed = result.contents.children[1]
                        restore_buffer_count = (malformed, int(malformed.contents.n_buffers))
                        malformed.contents.n_buffers = 1

                    def release(raw_node: object) -> None:
                        # release_type declares POINTER(Array); the same
                        # runtime-generated pointer boundary applies here.
                        node = cast(Any, raw_node)
                        try:
                            if snapshot_tree(node.contents) != expected:
                                failures.append("producer observed modified C Data headers")
                        except Exception as error:
                            failures.append(str(error))
                        finally:
                            releases.append(1)
                            if restore_buffer_count is not None:
                                malformed, original_buffers = restore_buffer_count
                                malformed.contents.n_buffers = original_buffers
                            original_release(node)

                    callback = release_type(release)
                    callbacks.append(callback)
                    result.contents.release = ctypes.cast(callback, ctypes.c_void_p).value
                    expected = snapshot_tree(result.contents)
                return int(status)

            callback = get_next_type(next_array)
            callbacks.append(callback)
            stream.contents.get_next = ctypes.cast(callback, ctypes.c_void_p).value
            return capsule

        return export

    for provider in (pl.Series, pl.DataFrame):
        monkeypatch.setattr(provider, "__arrow_c_stream__", wrap_export(provider.__arrow_c_stream__))
    if kind == "root":
        source = pl.Series([None], dtype=pl.Null)
    elif kind == "nested":
        source = pl.Series([[None]], dtype=pl.List(pl.Null))
    elif kind == "import":
        source = pl.DataFrame({"nulls": pl.Series([None], dtype=pl.Null), "bools": pl.Series([None], dtype=pl.Boolean)})
    else:
        source = pl.DataFrame({"nulls": pl.Series([None], dtype=pl.Null), "ints": [1]})
    if kind == "import":
        with pytest.raises(TypeError, match="Expected 1 buffers"):
            _sqlaero._encode_value(source)
    elif fail:
        # The first value imports/releases its live C tree before the next
        # value raises; owner cleanup must not depend on successful binding.
        with pytest.raises(UnicodeEncodeError):
            _sqlaero._bind_sql("SELECT %s,%s", [source, "🙂"], charset="gbk")
    else:
        _sqlaero._encode_value(source)
    assert failures == []
    assert releases == [1]


@pytest.mark.parametrize("mode", [
    "success", "early-schema-failure", "repeat-consumption",
    "cached-dictionary-success", "cached-dictionary-encoding-failure",
])
def test_c_array_consumption_owns_both_releases_on_every_exit(
    monkeypatch: pytest.MonkeyPatch, mode: str
) -> None:
    """Observe real C Array exports through a public, replaceable producer."""
    pa = pytest.importorskip("pyarrow", reason="C Array release observation needs both optional providers")

    class Schema(ctypes.Structure):
        _fields_ = [
            ("format", ctypes.c_void_p), ("name", ctypes.c_void_p), ("metadata", ctypes.c_void_p),
            ("flags", ctypes.c_int64), ("n_children", ctypes.c_int64), ("children", ctypes.c_void_p),
            ("dictionary", ctypes.c_void_p), ("release", ctypes.c_void_p), ("private_data", ctypes.c_void_p),
        ]

    class Array(ctypes.Structure):
        _fields_ = [
            ("length", ctypes.c_int64), ("null_count", ctypes.c_int64), ("offset", ctypes.c_int64),
            ("n_buffers", ctypes.c_int64), ("n_children", ctypes.c_int64), ("buffers", ctypes.c_void_p),
            ("children", ctypes.c_void_p), ("dictionary", ctypes.c_void_p),
            ("release", ctypes.c_void_p), ("private_data", ctypes.c_void_p),
        ]

    cached = mode.startswith("cached-dictionary-")
    if cached:
        failing = mode == "cached-dictionary-encoding-failure"
        labels = ["a'\\", "🙂"] if failing else ["a'\\"]
        codes = [0] * 39 + [int(failing)]
        source = pa.DictionaryArray.from_arrays(pa.array(codes, type=pa.int8()), pa.array(labels))
        value = pl.Series([labels[code] for code in codes], dtype=pl.Categorical)
    else:
        source = pa.array([1.5], type=pa.float16()) if mode == "early-schema-failure" else pa.array([7])
        value = pl.Series([7], dtype=pl.Int64)
    capsules = source.__arrow_c_array__()
    get_pointer = ctypes.pythonapi.PyCapsule_GetPointer
    get_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
    get_pointer.restype = ctypes.c_void_p
    release_type = ctypes.PYFUNCTYPE(None, ctypes.c_void_p)
    callbacks: list[object] = []
    releases: list[str] = []
    failures: list[str] = []

    def observe(capsule: object, name: bytes, header: type[ctypes.Structure], kind: str) -> None:
        # The external ABI is declared above. Only this dynamic C pointer is
        # cast to Any; every byte/header and release count remains asserted.
        pointer = cast(Any, ctypes.cast(get_pointer(capsule, name), ctypes.POINTER(header)))
        original_address = pointer.contents.release
        original = release_type(original_address)
        expected = bytes(pointer.contents)

        def released(raw_pointer: int) -> None:
            live = cast(Any, ctypes.cast(raw_pointer, ctypes.POINTER(header)))
            live.contents.release = original_address
            if bytes(live.contents) != expected:
                failures.append(kind + " header changed before original release")
            releases.append(kind)
            original(raw_pointer)

        callback = release_type(released)
        callbacks.append(callback)
        pointer.contents.release = ctypes.cast(callback, ctypes.c_void_p).value

    observe(capsules[0], b"arrow_schema", Schema, "schema")
    observe(capsules[1], b"arrow_array", Array, "array")

    def export(_source: object, _requested_schema: object = None) -> object:
        return capsules

    monkeypatch.setattr(pl.Series, "__arrow_c_array__", export, raising=False)
    if mode == "early-schema-failure":
        with pytest.raises(TypeError, match="unsupported Arrow"):
            _sqlaero._encode_value(value)
    elif mode == "cached-dictionary-encoding-failure":
        with pytest.raises(UnicodeEncodeError) as caught:
            _sqlaero._bind_sql("SELECT %s", (value,), charset="ascii")
        assert caught.value.object == "🙂"
    else:
        expected = b"(" + b",".join([b"'a\\'\\\\'"] * 40) + b")" if cached else b"(7)"
        assert _sqlaero._encode_value(value).to_bytes() == expected
        if mode in ("repeat-consumption", "cached-dictionary-success"):
            with pytest.raises(TypeError, match="already consumed"):
                _sqlaero._encode_value(value)
    assert sorted(releases) == ["array", "schema"]
    assert failures == []


def test_polars_object_values_are_never_exported_as_pointer_binary() -> None:
    value = pl.Series("objects", [1, None, "a", Decimal("1.25")], dtype=pl.Object)
    assert _sqlaero._encode_value(value).to_bytes() == b"(1,NULL,'a',1.25)"
    frame = pl.DataFrame([pl.Series("native", [0, 1, 2, 3]), value])
    assert _sqlaero._bind_sql("SELECT %s,%s", frame) == [
        b"\x03SELECT 0,1", b"\x03SELECT 1,NULL", b"\x03SELECT 2,'a'", b"\x03SELECT 3,1.25"
    ]


def test_polars_object_rejects_excluded_types_through_common_escape() -> None:
    with pytest.raises(TypeError):
        _sqlaero._encode_value(pl.Series([bytearray(b"x")], dtype=pl.Object))


def test_polars_object_cycle_preserves_original_root_identity() -> None:
    cell: list[object] = []
    value = pl.Series([cell], dtype=pl.Object)
    cell.append(value)
    with pytest.raises(_sqlaero.DataError):
        _sqlaero._encode_value(value)


@pytest.mark.parametrize("dtype", [pl.Int128, pl.List(pl.Int128)])
def test_polars_unsupported_schema_fails_even_when_empty(dtype: object) -> None:
    with pytest.raises(TypeError):
        _sqlaero._encode_value(pl.Series([], dtype=dtype))


def test_polars_native_path_does_not_need_pyarrow_python_package() -> None:
    result = subprocess.run(
        [sys.executable, "-c", "\n".join([
            "import sys",
            "sys.modules['pyarrow'] = None",
            "import polars as pl",
            "from sqlaero import _sqlaero",
            "assert _sqlaero._encode_value(pl.Series([1,None,3])).to_bytes() == b'(1,NULL,3)'",
            "assert _sqlaero._bind_sql('SELECT %s',pl.DataFrame({'x':[1,2]})) == [b'\\x03SELECT 1',b'\\x03SELECT 2']",
            "assert sys.modules.get('pyarrow') is None",
        ])],
        check=False,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
