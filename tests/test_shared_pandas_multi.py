"""MultiIndex native rows share the ordinary one-dimensional root policy."""

# pyright: reportPrivateUsage=false
from typing import Any


import json
import math
import pytest
from sqlaero import _sqlaero, json_dumps

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")


@pytest.mark.parametrize("length", [0, 2, 40])
@pytest.mark.parametrize("reverse", [False, True])
def test_actual_multiindex_rows_match_nested_values_and_many(length: int, reverse: bool) -> None:
    source = pd.MultiIndex(levels=[pd.Index([7, 8]), pd.Index(["ab", "cd"], dtype=object)], codes=[[n % 2 for n in range(length)], [(n + 1) % 2 for n in range(length)]])
    expected = [[7 if n % 2 == 0 else 8, "cd" if n % 2 == 0 else "ab"] for n in range(length)]
    if reverse:
        source, expected = source[::-1], expected[::-1]
    assert json.loads(json_dumps(source)) == expected
    projected = [(str(a), repr(b)) for a, b in expected] if expected else [()]
    assert _sqlaero._escape_projection(source, many=True) == projected


@pytest.mark.parametrize("length", [2, 40])
def test_missing_codes_are_actual_null_even_with_strict_nonfinite_option(length: int) -> None:
    source = pd.MultiIndex(levels=[pd.Index([7]), pd.Index(["ab"], dtype=object)], codes=[[-1, 0] * (length // 2), [0, -1] * (length // 2)])
    assert json.loads(json_dumps(source, allow_nan=False)) == [[None, "ab"], [7, None]] * (length // 2)


def test_actual_temporal_labels_keep_precision_and_missing_values() -> None:
    timestamps = pd.DatetimeIndex(["2026-11-12T08:09:10.123456789", "NaT"])
    durations = pd.TimedeltaIndex([pd.Timedelta(-1_123_456_900, unit="ns"), pd.NaT])
    source = pd.MultiIndex(
        levels=[timestamps, durations],
        codes=[[0, 1, 0], [0, 0, 1]],
        verify_integrity=False,
    )

    assert json_dumps(source, allow_nan=False) == (
        b'[["2026-11-12T08:09:10.123456789","-PT1.1234569S"],'
        b'[null,"-PT1.1234569S"],["2026-11-12T08:09:10.123456789",null]]'
    )
    assert _sqlaero._escape_projection(source, many=True) == [
        ("'2026-11-12 08:09:10.123456'", "'-00:00:01.123456'"),
        ("NULL", "'-00:00:01.123456'"),
        ("'2026-11-12 08:09:10.123456'", "NULL"),
    ]


def test_numeric_rows_do_not_construct_python_index_scalars_or_tuples(monkeypatch: pytest.MonkeyPatch) -> None:
    source = pd.MultiIndex(levels=[pd.Index([2**64 - 1], dtype="uint64"), pd.Index([-0.0, 0.5], dtype="float64")], codes=[[0] * 40, [n % 2 for n in range(40)]])
    def forbidden(*args: object, **kwargs: object):
        raise AssertionError("unneeded Python Index scalar/tuple getter ran")
    monkeypatch.setattr(pd.Index, "__getitem__", forbidden)
    monkeypatch.setattr(pd.MultiIndex, "__getitem__", forbidden)
    result = json.loads(json_dumps(source))
    assert result[0][0] == 2**64 - 1
    assert math.copysign(1.0, result[0][1]) == -1.0
    assert result[1][1] == 0.5


def test_real_multiindex_and_level_subclasses_read_actual_base_storage() -> None:
    source = pd.MultiIndex.from_tuples([(7, "ab"), (8, "cd")])
    class ActualMulti(pd.MultiIndex):
        def __getitem__(self, value: Any):
            raise AssertionError("replacement tuple getter ran")
        @property
        def values(self):
            raise AssertionError("replacement values ran")
    source.__class__ = ActualMulti
    assert json_dumps(source) == b'[[7,"ab"],[8,"cd"]]'


def test_large_level_count_is_not_truncated_to_old_fast_path_width() -> None:
    source = pd.MultiIndex(levels=[pd.Index([n]) for n in range(20)], codes=[[0] * 3 for _ in range(20)])
    assert json.loads(json_dumps(source)) == [list(range(20))] * 3
    assert _sqlaero._escape_projection(source, many=True) == [tuple(str(n) for n in range(20))] * 3


def test_unusual_public_negative_code_keeps_index_value_and_other_real_null() -> None:
    source = pd.MultiIndex(levels=[pd.Index([10, 20, 30]), pd.Index(["ab"])], codes=[[-2, 0], [-1, 0]], verify_integrity=False)
    assert json_dumps(source, allow_nan=False) == b'[[20,null],[10,"ab"]]'


def test_wide_codes_cross_an_owned_block_with_null_and_negative_index() -> None:
    codes = [255, -1, -2, 254] * 259
    source = pd.MultiIndex(
        levels=[pd.RangeIndex(400)],
        codes=[codes],
        verify_integrity=False,
    )[::-1]
    assert source.codes[0].dtype.itemsize == 2
    expected = [[254], [398], [None], [255]] * 259
    assert json.loads(json_dumps(source)) == expected
    assert _sqlaero._escape_projection(source, many=True) == [
        ("254",), ("398",), ("NULL",), ("255",)
    ] * 259


def test_out_of_range_code_remains_original_standard_index_error() -> None:
    source = pd.MultiIndex(levels=[pd.Index([7])], codes=[[0, 4]], verify_integrity=False)
    with pytest.raises(IndexError, match="code 4"):
        json_dumps(source)


def test_native_arrow_labels_can_revisit_chunks_in_a_multiindex() -> None:
    pa = pytest.importorskip("pyarrow")
    strings = pd.Index(pd.array(pa.chunked_array([pa.array(["ab"]), pa.array(["cd", "ef"])]), dtype=pd.ArrowDtype(pa.string())))
    source = pd.MultiIndex(levels=[strings, pd.RangeIndex(7, 9)], codes=[[2, 0, 1, 2, 0] * 8, [0, 1, 0, 1, 0] * 8])
    assert json.loads(json_dumps(source)) == [["ef", 7], ["ab", 8], ["cd", 7], ["ef", 8], ["ab", 7]] * 8
