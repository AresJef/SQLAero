"""Categories reuse the module-owned reader and preserve actual label values."""

import json
from datetime import datetime, timedelta, tzinfo
import pytest
from sqlaero import _sqlaero, json_dumps

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")


@pytest.mark.parametrize("dtype", ["int64", "uint64", "float64", "string[python]", "string[pyarrow]"])
def test_native_labels_keep_random_codes_missing_and_actual_values(dtype: str) -> None:
    labels = [7, 8, 9] if "64" in dtype else ["ab", "cd", "ef"]
    index = pd.Index(labels, dtype=dtype)
    codes = [2, 0, -1, 1, 2, 0] * 8
    value = pd.Categorical.from_codes(codes, categories=index)
    assert json.loads(json_dumps(value)) == [None if n < 0 else labels[n] for n in codes]


def test_arrow_labels_revisit_previous_chunks_without_python_value_conversion() -> None:
    pa = pytest.importorskip("pyarrow")
    labels = pa.chunked_array([pa.array(["ab"]), pa.array([], type=pa.string()), pa.array(["cd", "ef"])])
    index = pd.Index(pd.array(labels, dtype=pd.ArrowDtype(pa.string())))
    codes = [2, 0, 1, -1, 2, 0] * 8
    value = pd.Categorical.from_codes(codes, categories=index)
    assert json.loads(json_dumps(value)) == [None if n < 0 else ["ab", "cd", "ef"][n] for n in codes]
    assert _sqlaero._escape_projection(value, many=True) == ["NULL" if n < 0 else ["'ab'", "'cd'", "'ef'"][n] for n in codes]


def test_composite_object_labels_use_ordinary_selected_row_rules() -> None:
    labels = pd.Index([(7, "ab"), (8, "cd")], tupleize_cols=False, dtype=object)
    value = pd.Categorical.from_codes([1, 0, -1], categories=labels)
    assert json_dumps(value) == b'[[8,"cd"],[7,"ab"],null]'
    assert _sqlaero._escape_projection(value, many=True) == [("8", "'cd'"), ("7", "'ab'"), "NULL"]
    assert _sqlaero._escape_projection(value) == ("(8,'cd')", "(7,'ab')", "NULL")


def test_all_missing_or_bad_first_code_does_not_acquire_labels(monkeypatch: pytest.MonkeyPatch) -> None:
    original = RuntimeError("unused label array was requested")
    value = pd.Categorical.from_codes([-1, -1], categories=pd.Index([7, 8]))
    def forbidden(self):
        raise original
    monkeypatch.setattr(pd.Index, "array", property(forbidden))
    assert json_dumps(value, allow_nan=False) == b"[null,null]"
    assert _sqlaero._escape_projection(value, many=True) == ["NULL", "NULL"]


def test_label_subclass_replacement_getters_do_not_replace_actual_values() -> None:
    class ActualIndex(pd.Index):
        def __getitem__(self, index):
            raise AssertionError("replacement label getter ran")
        @property
        def array(self):
            raise AssertionError("replacement label storage ran")
    labels = pd.Index([7, 8])
    value = pd.Categorical.from_codes([1, 0], categories=labels)
    value.categories.__class__ = ActualIndex
    assert json_dumps(value) == b"[8,7]"


def test_required_timezone_protocol_repeats_and_original_error_propagates() -> None:
    original = MemoryError("actual categorical datetime protocol")
    calls = []
    armed = False
    class Zone(tzinfo):
        def utcoffset(self, value):
            if armed:
                calls.append(value)
                if len(calls) == 3:
                    raise original
            return timedelta(0)
    source = datetime(2026, 11, 12, tzinfo=Zone())
    labels = pd.Index([source], dtype=object)
    value = pd.Categorical.from_codes([0, 0, 0, 0], categories=labels)
    calls.clear()
    armed = True
    with pytest.raises(MemoryError) as caught:
        json_dumps(value)
    assert caught.value is original
    assert len(calls) == 3
