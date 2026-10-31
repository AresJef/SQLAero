"""Standard container roles through the production incremental binder."""
# pyright: reportPrivateUsage=false

from collections import UserDict, UserList, deque
from collections.abc import Callable
import pytest
from sqlaero import _sqlaero


def containers():
    data = {"first": 11, "second": 22}
    return [data, data.values(), data.items(), {11: None, 22: None}.keys(), range(11, 33, 11)]


@pytest.mark.parametrize("value", containers())
def test_ordered_sequences_share_value_roles(value: object) -> None:
    assert _sqlaero._encode_value(value).to_bytes() == b"(11,22)"
    assert _sqlaero._bind_sql("SELECT %s,%s", value) == [b"\x03SELECT 11,22"]
    assert _sqlaero._bind_sql("SELECT %s", value, many=True) == [b"\x03SELECT 11", b"\x03SELECT 22"]
    assert _sqlaero._bind_sql("SELECT %s", value, itemize=False) == [b"\x03SELECT (11,22)"]


@pytest.mark.parametrize("constructor", [set, frozenset])
def test_unordered_sequences_follow_their_actual_iteration_order(constructor: type) -> None:
    value = constructor([31, 1, 91, 7])
    expected = b"(" + b",".join(str(x).encode("ascii") for x in value) + b")"
    assert _sqlaero._encode_value(value).to_bytes() == expected


def test_dict_items_uses_only_values_and_does_not_format_keys() -> None:
    class HostileKey:
        def __repr__(self) -> str:
            raise AssertionError("key repr must not run")
    value = {HostileKey(): (1, 2), HostileKey(): None}
    assert _sqlaero._encode_value(value.items()).to_bytes() == b"((1,2),NULL)"


@pytest.mark.parametrize("factory", [dict, set, frozenset, lambda: range(0), lambda: dict[object, object]().values(), lambda: dict[object, object]().items(), lambda: dict[object, object]().keys()])
def test_empty_ordinary_container_keeps_existing_many_and_itemize_rules(factory: Callable[[], object]) -> None:
    value = factory()
    assert _sqlaero._bind_sql("SELECT 1", value) == [b"\x03SELECT 1"]
    assert _sqlaero._bind_sql("SELECT %s", value, itemize=False) == [b"\x03SELECT ()"]
    assert _sqlaero._bind_sql("SELECT 1", value, many=True, itemize=False) == [b"\x03SELECT 1"]


def test_dict_and_list_cross_reference_reports_common_path() -> None:
    data: dict[str, object] = {}
    rows: list[object] = [data]
    data["value"] = rows
    with pytest.raises(Exception, match="true cycle") as caught:
        _sqlaero._bind_sql("SELECT %s", [rows])
    assert any("args[0][0][0]" in note for note in getattr(caught.value, "__notes__", ()))


def test_view_and_tuple_cross_reference_is_a_true_cycle() -> None:
    data: dict[str, object] = {}
    view = data.values()
    data["value"] = (view,)
    with pytest.raises(Exception, match="true cycle"):
        _sqlaero._encode_value(view)


def test_range_is_incremental_and_large_start_still_uses_numeric_policy() -> None:
    with pytest.raises(Exception):
        _sqlaero._bind_sql("SELECT %s", range(10**65, 10**65 + 1), many=True)
    assert _sqlaero._encode_value(range(3)).to_bytes() == b"(0,1,2)"


@pytest.mark.parametrize("value", [UserDict({"a": 1}), UserList([1]), deque([1]), iter([1]), (x for x in [1]), bytearray(b"abc"), memoryview(b"abc")])
def test_unlisted_iterables_and_excluded_binary_stay_unsupported(value: object) -> None:
    with pytest.raises(TypeError):
        _sqlaero._encode_value(value)
