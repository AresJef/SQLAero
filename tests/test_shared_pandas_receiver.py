"""Canonical provider receivers share real instance state on every ABI."""

# pyright: reportPrivateUsage=false
from typing import Any, NoReturn, cast

import pytest
from sqlaero import _sqlaero, json_dumps

pd=pytest.importorskip("pandas")


def source(kind: str) -> tuple[Any, bytes, object]:
    if kind=="multi":
        return pd.MultiIndex.from_tuples([(7,"ab"),(8,"cd")]), b'[[7,"ab"],[8,"cd"]]', ("(7,'ab')","(8,'cd')")
    if kind=="frame":
        return pd.DataFrame({"id":[7,8]}), b'[{"id":7},{"id":8}]', [("7",),("8",)]
    if kind=="series":
        return pd.Series([7,8]), b"[7,8]", ("7","8")
    if kind=="range":
        return pd.RangeIndex(7,9), b"[7,8]", ("7","8")
    return pd.Index([7,8]), b"[7,8]", ("7","8")


@pytest.mark.parametrize("kind",["multi","frame","series","range","index"])
@pytest.mark.parametrize("target",["json","mysql"])
def test_real_subclass_receiver_reads_shared_state_through_canonical_attribute_protocol(kind: str,target: str) -> None:
    value,document,sql=source(kind)
    base=cast(type[Any],type(value))
    child=type("StoredProvider",(base,),{"__slots__":()})
    value.__class__=child
    calls: list[str]=[]
    def forbidden(self: object,name: str) -> NoReturn:
        calls.append(name)
        raise AssertionError("replacement provider getter must not execute")
    child.__getattribute__=forbidden
    if target=="json":
        assert json_dumps(value)==document
    else:
        assert _sqlaero._escape_projection(value)==sql
    assert calls==[]


@pytest.mark.parametrize("length",[0,1])
@pytest.mark.parametrize("target",["json","mysql"])
def test_unsupported_derived_index_keeps_declared_schema_error_not_an_incomplete_receiver(length: int,target: str) -> None:
    value=pd.period_range("2024-01-01",periods=length,freq="D")
    if target=="json":
        with pytest.raises(TypeError,match="declared Pandas extension dtype"):json_dumps(value)
    else:
        with pytest.raises(TypeError,match="declared Pandas extension dtype"):_sqlaero._escape_projection(value)
