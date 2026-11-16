"""Structured masks preserve valid records and mask each actual field."""

# pyright: reportPrivateUsage=false


import json
from datetime import datetime, timedelta, tzinfo

import pytest

from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("shape", [(2,), (1, 2), (2, 1, 1)])
def test_record_masks_preserve_axes_and_each_field_position(shape: tuple[int, ...]) -> None:
    data = np.array([(7, "ab"), (8, "cd")], dtype=[("id", "i8"), ("name", "U2")])
    value = np.ma.array(data, mask=[(False, True), (True, False)]).reshape(shape)
    row = [{"id": 7, "name": None}, {"id": None, "name": "cd"}]
    expected = row if len(shape) == 1 else [row] if len(shape) == 2 else [[[row[0]]], [[row[1]]]]
    assert json.loads(json_dumps(value)) == expected
    if len(shape) == 1:
        assert _sqlaero._encode_value(value).to_bytes() == b"((7,NULL),(NULL,'cd'))"
    else:
        with pytest.raises(ValueError, match="axes"):
            _sqlaero._encode_value(value)


@pytest.mark.parametrize("mask", [False, True])
def test_scalar_structured_mask_remains_an_object(mask: bool) -> None:
    data = np.array((7, "ab"), dtype=[("id", "i8"), ("name", "U2")])
    value = np.ma.array(data, mask=mask)
    assert json_dumps(value) == (b'{"id":null,"name":null}' if mask else b'{"id":7,"name":"ab"}')
    assert _sqlaero._encode_value(value).to_bytes() == (b"(NULL,NULL)" if mask else b"(7,'ab')")


def test_fixed_subarray_and_nested_record_masks_do_not_flatten_fields() -> None:
    dtype = [("id", "i8"), ("xy", "i8", (2,)), ("child", [("x", "i8"), ("y", "U2")])]
    data = np.array([(7, [1, 2], (3, "ab"))], dtype=dtype)
    value = np.ma.array(data, mask=False)
    value.mask["xy"][0, 1] = True
    value.mask["child"]["x"][0] = True
    assert json_dumps(value) == b'[{"id":7,"xy":[1,null],"child":{"x":null,"y":"ab"}}]'
    assert _sqlaero._encode_value(value).to_bytes() == b"((7,(1,NULL),(NULL,'ab')))"


def test_structured_mask_hides_unsupported_object_payload_without_calling_it() -> None:
    data = np.array([(7, object())], dtype=[("id", "i8"), ("bad", object)])
    value = np.ma.array(data, mask=[(False, True)])
    assert json_dumps(value) == b'[{"id":7,"bad":null}]'
    assert _sqlaero._encode_value(value).to_bytes() == b"((7,NULL))"
    value.mask["bad"][0] = False
    with pytest.raises(TypeError):
        json_dumps(value)


def test_zero_records_and_zero_fields_are_still_distinct() -> None:
    assert json_dumps(np.ma.array(np.empty(0, dtype=[]), mask=False)) == b"[]"
    assert json_dumps(np.ma.array(np.empty(2, dtype=[]), mask=False)) == b"[{},{}]"
    assert json_dumps(np.ma.array(np.empty((), dtype=[]), mask=False)) == b"{}"


def test_structured_mask_does_not_bypass_target_schema() -> None:
    for value in [np.ma.array(np.empty(0, dtype=[("x", "S2")]), mask=False),
                  np.ma.array(np.zeros(1, dtype=[("x", "S2")]), mask=True)]:
        with pytest.raises(TypeError, match="binary"):
            json_dumps(value)
    with pytest.raises(TypeError):
        json_dumps(np.ma.array(np.zeros(1, dtype=[("x", "c16")]), mask=True))


def test_structured_mask_full_field_names_and_negative_strides() -> None:
    data = np.array([(7, 8), (9, 10)], dtype=[("a\0x", ">i8"), ("a\0y", "<i8")])
    value = np.ma.array(data, mask=[(False, True), (True, False)])[::-1]
    assert json_dumps(value) == b'[{"a\\u0000x":null,"a\\u0000y":10},{"a\\u0000x":7,"a\\u0000y":null}]'


def test_invalid_record_mask_field_names_are_rejected() -> None:
    value = np.ma.array(np.zeros(1, dtype=[("x", "i8")]), mask=False)
    value.__dict__["_mask"] = np.zeros(1, dtype=[("other", "?")])
    with pytest.raises(TypeError, match="field"):
        json_dumps(value)


def test_mask_name_comparison_keeps_different_nul_suffixes_distinct() -> None:
    value = np.ma.array(np.zeros(1, dtype=[("a\0x", "i8")]), mask=False)
    value.__dict__["_mask"] = np.zeros(1, dtype=[("a\0y", "?")])
    with pytest.raises(TypeError, match="field"):
        json_dumps(value)


@pytest.mark.parametrize("unit", ["ps", "generic", "0ns"])
@pytest.mark.parametrize("structured", [False, True])
def test_masked_finite_temporal_payload_needs_no_finite_unit_admission(unit: str, structured: bool) -> None:
    raw = np.array([1], dtype="i8").view(np.dtype("M8" if unit == "generic" else "M8[" + unit + "]"))
    if structured:
        data = np.empty(1, dtype=[("at", raw.dtype)])
        # Preserve the exact ticks without asking NumPy to convert a finite
        # temporal value through unsupported generic/zero-multiplier units.
        data["at"].view("i8")[:] = raw.view("i8")
    else:
        data = raw
    if structured and unit in ("generic", "0ns"):
        # NumPy2.5.4's default structured fill construction rejects these
        # units before SQLAero runs. The public same-width dtype assignment
        # produces the actual supported storage; its source deprecation is
        # preserved as a warning, not mistaken for a product conversion error.
        value = np.ma.array(np.zeros(1, dtype=[("at", "M8[ns]")]), mask=True)
        value.dtype = data.dtype
        value.data["at"].view("i8")[:] = 1
    else:
        value = np.ma.array(data, mask=True)
    assert json_dumps(value, allow_nan=False) == (b'[{"at":null}]' if structured else b"[null]")
    assert _sqlaero._encode_value(value).to_bytes() == (b"((NULL))" if structured else b"(NULL)")


@pytest.mark.parametrize("target", ["json", "mysql"])
@pytest.mark.parametrize("raise_original", [False, True])
def test_mask_replacement_during_a_record_field_callback_is_not_reused(target: str, raise_original: bool) -> None:
    original = MemoryError("original structured mask field protocol")
    data = np.empty((), dtype=[("at", object), ("hidden", object)])
    value = np.ma.array(data, mask=False)
    value.mask["hidden"] = True

    class Zone(tzinfo):
        def utcoffset(self, item: datetime | None) -> timedelta:
            value.__dict__["_mask"] = np.zeros((), dtype=[("at", "?"), ("hidden", "?")])
            if raise_original:
                raise original
            return timedelta(0)

    value.data["at"] = datetime(2026, 1, 2, tzinfo=Zone())
    value.data["hidden"] = object()
    with pytest.raises(MemoryError if raise_original else BufferError) as caught:
        if target == "json": json_dumps(value)
        else: _sqlaero._encode_value(value)
    if raise_original: assert caught.value is original
    else: assert "mask changed" in str(caught.value)


def test_real_structured_mask_subclass_keeps_actual_fields_and_masks() -> None:
    calls: list[str] = []

    class Child(np.ma.MaskedArray):
        pass

    value = np.ma.array(np.array([(7, "ab")], dtype=[("id", "i8"), ("name", "U2")]),
                        mask=[(False, True)]).view(Child)

    def forbidden(*args: object, **kwargs: object) -> object:
        calls.append("override")
        raise AssertionError("structured mask replacement hook called")

    for name in ["__getattribute__", "__getitem__", "view", "filled"]:
        setattr(Child, name, forbidden)
    for name in ["mask", "_mask", "data", "dtype", "base"]:
        setattr(Child, name, property(forbidden))
    assert json_dumps(value) == b'[{"id":7,"name":null}]'
    assert _sqlaero._encode_value(value).to_bytes() == b"((7,NULL))"
    assert calls == []
