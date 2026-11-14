"""All actual JSON object names share the approved native integer policy."""

import pytest
from sqlaero import json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize(("dtype", "value", "digits"), [
    (np.int8, -7, "-7"), (np.int16, -257, "-257"),
    (np.int32, -(2**31), "-2147483648"), (np.int64, -(2**63), "-9223372036854775808"),
    (np.uint8, 255, "255"), (np.uint16, 65535, "65535"),
    (np.uint32, 2**32 - 1, "4294967295"), (np.uint64, 2**64 - 1, "18446744073709551615"),
])
def test_supported_numpy_integer_key_facts_use_exact_decimal_names(dtype: type, value: int, digits: str) -> None:
    assert json_dumps({dtype(value): 7}) == ('{"' + digits + '":7}').encode()


@pytest.mark.parametrize("value", [np.int64(1), np.uint64(1)])
def test_numpy_integer_key_collision_matches_the_same_python_integer_name(value: object) -> None:
    with pytest.raises(ValueError, match="collid"):
        json_dumps({value: 7, "1": 8})


@pytest.mark.parametrize("value", [np.bool_(True), np.float32(1), np.float64(float("nan")), np.datetime64(1, "ns"), np.timedelta64(1, "ns")])
def test_key_type_rejection_does_not_execute_unneeded_temporal_protocol(value: object, monkeypatch: pytest.MonkeyPatch) -> None:
    def forbidden(*args: object, **kwargs: object) -> None:
        raise AssertionError("rejected key acquired temporal metadata")

    monkeypatch.setattr(np, "datetime_data", forbidden)
    with pytest.raises(TypeError):
        json_dumps({value: 7})
