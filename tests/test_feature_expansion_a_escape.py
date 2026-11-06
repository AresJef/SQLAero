"""Public SQL fragment projection uses the existing native row roles."""

# pyright: reportPrivateUsage=false

from typing import Any

import pytest
from sqlaero import _sqlaero


def escape(data: object, **options: Any) -> object:
    return getattr(_sqlaero, "_escape_projection")(data, **options)


@pytest.mark.parametrize("many", [False, True])
@pytest.mark.parametrize("itemize", [False, True])
@pytest.mark.parametrize(("data", "expected"), [(42, "42"), (None, "NULL"), ("", "''"), (b"", "CAST(X'' AS BINARY)")])
def test_root_scalars_remain_strings(data: object, expected: str, many: bool, itemize: bool) -> None:
    assert escape(data, many=many, itemize=itemize) == expected


def test_sequence_and_batch_projection_preserves_explicit_rows() -> None:
    assert escape([1, 2]) == ("1", "2")
    assert escape([1, 2], itemize=False) == "(1,2)"
    assert escape([1, 2], many=True) == ["1", "2"]
    assert escape([(1,), (2,)], many=True) == [("1",), ("2",)]
    assert escape([(1, 2), (3, 4)], many=True) == [("1", "2"), ("3", "4")]
    assert escape([(1, 2), (3, 4)], many=True, itemize=False) == ["(1,2)", "(3,4)"]
    assert escape([[1, 2], [3, 4]], itemize=False) == "((1,2),(3,4))"
    empty_rows: list[list[object]] = [[]]
    assert escape(empty_rows, many=True) == [()]
    assert escape(empty_rows, many=True, itemize=False) == ["()"]


@pytest.mark.parametrize("root", [[], ()])
def test_empty_ordinary_root_preserves_synthetic_zero_positions(root: object) -> None:
    assert escape(root) == ()
    assert escape(root, itemize=False) == "()"
    assert escape(root, many=True) == [()]
    assert escape(root, many=True, itemize=False) == [()]


def test_native_empty_and_zero_column_roles_are_not_empty_batches() -> None:
    np: Any = pytest.importorskip("numpy")
    for itemize in (False, True):
        assert escape(np.empty(0), itemize=itemize) == ()
        assert escape(np.empty(0), many=True, itemize=itemize) == [()]
        for shape in ((0, 0), (0, 2)):
            assert escape(np.empty(shape), itemize=itemize) == [()]
            assert escape(np.empty(shape), many=True, itemize=itemize) == [()]
        assert escape(np.empty((2, 0)), itemize=itemize) == [(), ()]


def test_natural_table_and_record_projection_uses_shared_shapes() -> None:
    np: Any = pytest.importorskip("numpy")
    pd: Any = pytest.importorskip("pandas")

    for many in (False, True):
        for table in (np.array([[1, 2], [3, 4]]), pd.DataFrame({"a": [1, 3], "b": [2, 4]})):
            assert escape(table, many=many) == [("1", "2"), ("3", "4")]
            assert escape(table, many=many, itemize=False) == ["(1,2)", "(3,4)"]
        record = np.array([(1, 2)], dtype=[("z", "i4"), ("a", "i4")])[0]
        assert escape(record, many=many) == ("1", "2")
        assert escape(record, many=many, itemize=False) == "(1,2)"
        empty_record = np.empty(1, dtype=[])[0]
        assert escape(empty_record, many=many) == ()
        assert escape(empty_record, many=many, itemize=False) == "()"
    assert escape([record], many=True) == [("1", "2")]


def test_arrow_natural_table_projection_uses_shared_shapes() -> None:
    pa: Any = pytest.importorskip("pyarrow")
    for many in (False, True):
        table = pa.table({"a": [1, 3], "b": [2, 4]})
        assert escape(table, many=many) == [("1", "2"), ("3", "4")]
        assert escape(table, many=many, itemize=False) == ["(1,2)", "(3,4)"]


@pytest.mark.parametrize(("charset", "text"), [("utf8mb4", "中😀'\\"), ("utf8mb3", "中\uffff'\\"),
                                                ("latin1", "é'\\"), ("gbk", "中丂'\\"), ("sjis", "表〜\\")])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_final_text_roundtrips_the_exact_writer_bytes(charset: str, text: str, no_backslash: bool) -> None:
    result = escape(text, charset=charset, no_backslash=no_backslash)
    original = getattr(_sqlaero, "_encode_scalar")(text, charset=charset, no_backslash=no_backslash)
    # Re-encoding unquoted SQL uses the same source codec and preserves every
    # literal syntax byte, including multibyte trail backslashes.
    rebound = getattr(_sqlaero, "_bind_sql")(result, (), charset=charset, no_backslash=no_backslash)
    assert rebound == [b"\x03" + original]


def test_sjis_reports_only_the_specific_nonlossless_auxiliary_text() -> None:
    assert escape("表", charset="sjis") == "'表'"
    assert escape("\\", charset="sjis") == "'\\'"
    assert escape("'", charset="sjis", no_backslash=True) == "''''"
    with pytest.raises(ValueError, match="charset sjis.*losslessly.*no SQL was executed"):
        escape("'", charset="sjis")
    # Input values still use the original canonical encoder and SQL escaping.
    assert getattr(_sqlaero, "_encode_scalar")("'", charset="sjis") == b"'\\''"


def test_auxiliary_roundtrip_validation_keeps_bounded_long_value_checkpoints() -> None:
    text = "é" * 300_000
    assert escape(text, charset="latin1") == "'" + text + "'"
    assert escape("表" * 200_000, charset="sjis") == "'" + "表" * 200_000 + "'"
    with pytest.raises(ValueError, match="charset sjis.*losslessly"):
        escape("表" * 200_000 + "'", charset="sjis")


class ProtocolString(str):
    def __str__(self) -> str:
        return "adapted'value"

    def __len__(self) -> int:
        raise AssertionError("must read base string length")


def test_final_fragment_is_exact_string_from_the_existing_string_protocol() -> None:
    value = escape(ProtocolString("base value"))
    assert type(value) is str and value == "'adapted\\'value'"


def test_escape_preserves_the_original_string_protocol_exception() -> None:
    original = MemoryError("original value protocol failure")
    class Broken(str):
        def __str__(self) -> str:
            raise original
    with pytest.raises(MemoryError) as failure:
        escape([1, Broken("base")], many=True)
    assert failure.value is original


@pytest.mark.parametrize("charset", [
    "armscii8", "ascii", "big5", "binary", "cp1250", "cp1251", "cp1256", "cp1257",
    "cp850", "cp852", "cp866", "cp932", "dec8", "eucjpms", "euckr", "gb18030",
    "gb2312", "gbk", "geostd8", "greek", "hebrew", "hp8", "keybcs2", "koi8r",
    "koi8u", "latin1", "latin2", "latin5", "latin7", "macce", "macroman", "sjis",
    "swe7", "tis620", "ujis", "utf8mb3", "utf8mb4",
])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_sql_syntax_bytes_roundtrip_in_every_available_charset(charset: str, no_backslash: bool) -> None:
    text = "quote' double\" control\n\r\x00\x1a percent%s"
    if charset == "sjis" and not no_backslash:
        with pytest.raises(ValueError, match="sjis.*losslessly"):
            escape(text, charset=charset)
        return
    result = escape(text, charset=charset, no_backslash=no_backslash)
    encoded = getattr(_sqlaero, "_encode_scalar")(text, charset=charset, no_backslash=no_backslash)
    assert getattr(_sqlaero, "_bind_sql")(result, (), charset=charset, no_backslash=no_backslash) == [b"\x03" + encoded]
