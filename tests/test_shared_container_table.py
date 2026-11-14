"""Independent container examples from types 013-022 of the approved table."""

from collections import namedtuple
from decimal import Decimal
import json
import math
from uuid import UUID

import pytest

from sqlaero import _sqlaero, json_dumps
from sqlaero.errors import DataError, SQLAeroError


Point = namedtuple("Point", "x y")
Box = namedtuple("Box", "payload")
Empty = namedtuple("Empty", "")


@pytest.mark.parametrize(("value", "sql", "document"), [
    pytest.param([7, 8], ("7", "8"), b"[7,8]", id="013-01"),
    pytest.param([7, None], ("7", "NULL"), b"[7,null]", id="013-02"),
    pytest.param([7, [8, 9]], ("7", "(8,9)"), b"[7,[8,9]]", id="013-03"),
    pytest.param([], (), b"[]", id="013-04"),
    pytest.param((7, 8), ("7", "8"), b"[7,8]", id="014-01"),
    pytest.param((7,), ("7",), b"[7]", id="014-02"),
    pytest.param((), (), b"[]", id="014-03"),
    pytest.param(([7], [7]), ("(7)", "(7)"), b"[[7],[7]]", id="014-04"),
    pytest.param(Point(7, 8), ("7", "8"), b'{"x":7,"y":8}', id="015-01"),
    pytest.param(Box({"x": 7, "y": 8}), ("(7,8)",), b'{"payload":{"x":7,"y":8}}', id="015-03"),
    pytest.param(Empty(), (), b"{}", id="015-05"),
    pytest.param({"id": 7, "name": "ab"}, ("7", "'ab'"), b'{"id":7,"name":"ab"}', id="016-01"),
    pytest.param({}, (), b"{}", id="016-02"),
    pytest.param({7: "ab"}, ("'ab'",), b'{"7":"ab"}', id="016-03"),
    pytest.param({UUID("12345678-1234-5678-1234-567812345678"): 7}, ("7",),
                 b'{"12345678-1234-5678-1234-567812345678":7}', id="016-06"),
    pytest.param({"a": 7, "A": 8}, ("7", "8"), b'{"a":7,"A":8}', id="016-11"),
    pytest.param({"": 7}, ("7",), b'{"":7}', id="016-15"),
    pytest.param({"007": 7, 7: 8}, ("7", "8"), b'{"007":7,"7":8}', id="016-16"),
    pytest.param({"a": 7, "b": 8}.keys(), ("'a'", "'b'"), b'["a","b"]', id="017-01"),
    pytest.param({True: 7}.keys(), ("1",), b"[true]", id="017-02"),
    pytest.param({}.keys(), (), b"[]", id="017-03"),
    pytest.param({"a": 7, "b": 8}.values(), ("7", "8"), b"[7,8]", id="018-01"),
    pytest.param({"a": None}.values(), ("NULL",), b"[null]", id="018-02"),
    pytest.param({}.values(), (), b"[]", id="018-03"),
    pytest.param({"a": 7, "b": 8}.items(), ("7", "8"), b'[["a",7],["b",8]]', id="019-01"),
    pytest.param({True: 7}.items(), ("7",), b"[[true,7]]", id="019-02"),
    pytest.param({}.items(), (), b"[]", id="019-03"),
    pytest.param({None: 7}.items(), ("7",), b"[[null,7]]", id="019-06"),
    pytest.param(range(3), ("0", "1", "2"), b"[0,1,2]", id="020-01"),
    pytest.param(range(5, 0, -2), ("5", "3", "1"), b"[5,3,1]", id="020-02"),
    pytest.param(range(0), (), b"[]", id="020-03"),
])
def test_container_table_exact_target_exits(value: object, sql: object, document: bytes) -> None:
    assert _sqlaero._escape_projection(value) == sql
    assert json_dumps(value) == document


@pytest.mark.parametrize("base", [list, tuple], ids=["013-08", "014-06"])
def test_container_table_many_selects_only_the_outer_row(base: type) -> None:
    value = base([(7, "ab"), (8, "cd")])
    assert _sqlaero._escape_projection(value) == ("(7,'ab')", "(8,'cd')")
    assert _sqlaero._escape_projection(value, many=True) == [("7", "'ab'"), ("8", "'cd'")]
    assert json_dumps(value) == b'[[7,"ab"],[8,"cd"]]'


def test_named_record_table_many_is_a_root_sequence_rule() -> None:
    assert _sqlaero._escape_projection(Point(7, 8), many=True) == ["7", "8"]
    assert json_dumps(Point(7, 8)) == b'{"x":7,"y":8}'


@pytest.mark.parametrize("key", [True, 1.5, None, math.nan, ("a", "b"), math.inf, Decimal("NaN")],
                         ids=["016-05", "016-07", "016-08", "016-09", "016-10", "016-17", "016-18"])
def test_dictionary_table_unconsumed_sql_key_does_not_gain_json_permission(key: object) -> None:
    assert _sqlaero._escape_projection({key: 7}) == ("7",)
    for allow_nan in (False, True):
        with pytest.raises(TypeError):
            json_dumps({key: 7}, allow_nan=allow_nan)


def test_dictionary_table_full_key_policy_does_not_mutate_source_mapping() -> None:
    value = {1: "a", "1": "b"}
    assert _sqlaero._escape_projection(value) == ("'a'", "'b'")
    with pytest.raises(ValueError):
        json_dumps(value)
    assert list(value.items()) == [(1, "a"), ("1", "b")]
    source = {"a": 7, "a": 8}
    assert _sqlaero._escape_projection(source) == ("8",)
    assert json_dumps(source) == b'{"a":8}'
    assert _sqlaero._escape_projection({"\ud800": 7}) == ("7",)
    with pytest.raises(UnicodeEncodeError):
        json_dumps({"\ud800": 7})


def test_items_table_many_projects_values_and_preserves_empty_call() -> None:
    assert _sqlaero._escape_projection({"a": 7, "b": 8}.items(), many=True) == ["7", "8"]
    for itemize in (False, True):
        assert _sqlaero._escape_projection({}.items(), many=True, itemize=itemize) == [()]
    assert _sqlaero._escape_projection({b"a": 7}.items()) == ("7",)
    with pytest.raises(TypeError):
        json_dumps({b"a": 7}.items())


def test_range_table_applies_the_numeric_limit_only_to_mysql() -> None:
    source = range(10**65, 10**65 + 1)
    assert json_dumps(source) == b"[1" + b"0" * 65 + b"]"
    with pytest.raises(DataError):
        _sqlaero._escape_projection(source)


@pytest.mark.parametrize("base", [set, frozenset], ids=["021", "022"])
def test_set_table_uses_real_iteration_without_sorting_or_flattening(base: type) -> None:
    source = base([7, 8])
    actual_order = list(base.__iter__(source))
    assert _sqlaero._escape_projection(source) == tuple(str(v) for v in actual_order)
    assert json.loads(json_dumps(source)) == actual_order
    assert _sqlaero._escape_projection(base()) == ()
    assert json_dumps(base()) == b"[]"
    assert _sqlaero._escape_projection(base([(7, 8)])) == ("(7,8)",)
    assert json_dumps(base([(7, 8)])) == b"[[7,8]]"
    assert _sqlaero._escape_projection(base([b"ab"])) == ("CAST(X'6162' AS BINARY)",)
    with pytest.raises(TypeError):
        json_dumps(base([b"ab"]))
    assert _sqlaero._escape_projection(base([math.nan])) == ("NULL",)
    assert json_dumps(base([math.nan])) == b"[null]"
    with pytest.raises(ValueError):
        json_dumps(base([math.nan]), allow_nan=False)


def test_container_table_invalid_leaf_and_cycle_keep_real_locations() -> None:
    for convert in (_sqlaero._escape_projection, json_dumps):
        with pytest.raises(TypeError) as caught:
            convert([7, bytearray(b"ab")])
        assert "[1]" in "\n".join(caught.value.__notes__)
    source = [7]
    source.append(source)
    with pytest.raises(SQLAeroError, match="cycle"):
        _sqlaero._escape_projection(source)
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(source)
    # An exception cannot leave ancestry in the next independent invocation.
    assert _sqlaero._escape_projection([7, [8]]) == ("7", "(8)")
    assert json_dumps([7, [8]]) == b"[7,[8]]"


@pytest.mark.parametrize(("factory", "document"), [
    (list, b"[]"), (tuple, b"[]"), (dict, b"{}"), (set, b"[]"),
    (frozenset, b"[]"), (lambda: range(0), b"[]"), (Empty, b"{}"),
    (lambda: {}.keys(), b"[]"), (lambda: {}.values(), b"[]"), (lambda: {}.items(), b"[]"),
])
def test_empty_container_table_root_keeps_one_zero_parameter_call(factory, document) -> None:
    source = factory()
    assert _sqlaero._escape_projection(source) == ()
    assert _sqlaero._escape_projection(source, itemize=False) == "()"
    for itemize in (False, True):
        assert _sqlaero._escape_projection(source, many=True, itemize=itemize) == [()]
    assert json_dumps(source) == document
