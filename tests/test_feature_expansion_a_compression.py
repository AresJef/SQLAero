"""Immutable compression configuration and shorthand share one native parser."""

from enum import IntEnum
import inspect
from typing import Any

import pytest
from sqlaero import _sqlaero


def configured(**values: Any) -> Any:
    return getattr(_sqlaero, "Compression")(**values)


def prepared(value: object = None) -> tuple[tuple[str, ...], int]:
    return getattr(_sqlaero, "_parse_compression")(value)


@pytest.mark.parametrize(("value", "names"), [
    (None, ("uncompressed",)), (" ZSTD, zlib ,ZSTD ", ("zlib", "zstd")),
    (["ZSTD", " zlib ", "zstd"], ("zlib", "zstd")),
    (("uncompressed", "ZSTD", "zlib"), ("zlib", "zstd", "uncompressed")),
    ("\t ZsTd\r\n", ("zstd",)),
])
def test_class_and_shorthand_prepare_the_same_native_permissions(value: object, names: tuple[str, ...]) -> None:
    config = configured(algorithms=value)
    assert config.algorithms == names and type(config.algorithms) is tuple
    assert config.algorithms is config.algorithms
    assert prepared(value) == prepared(config) == (names, 3)
    assert config.zstd_compression_level == (3 if "zstd" in names else None)


def test_constructor_requires_keyword_algorithm_and_keeps_exact_default_level() -> None:
    cls = getattr(_sqlaero, "Compression")
    for values in ({}, {"zstd_compression_level": 10}):
        with pytest.raises(TypeError):
            cls(**values)
    with pytest.raises(TypeError):
        cls("zstd")
    signature = inspect.signature(cls)
    assert signature.parameters["algorithms"].kind is inspect.Parameter.KEYWORD_ONLY
    assert signature.parameters["algorithms"].default is inspect.Parameter.empty
    assert signature.parameters["zstd_compression_level"].default == 3


class ProtocolTrap:
    def __str__(self) -> str:
        raise AssertionError("configuration must not invoke __str__")
    def __iter__(self) -> Any:
        raise AssertionError("configuration must not invoke __iter__")
    def __index__(self) -> int:
        raise AssertionError("configuration must not invoke __index__")
    def __int__(self) -> int:
        raise AssertionError("configuration must not invoke __int__")


@pytest.mark.parametrize("value", [True, 1, 1.0, b"zstd", {"zstd"}, {"algorithm": "zstd"},
                                  ProtocolTrap(), [None], [1], [True], [["zstd"]]])
def test_algorithm_types_are_rejected_without_conversion_hooks(value: object) -> None:
    for factory in (lambda: configured(algorithms=value), lambda: prepared(value)):
        with pytest.raises(TypeError):
            factory()


@pytest.mark.parametrize("value", ["", " ", [], (), "zstd,,zlib", ",zstd", "zstd,", "off", "auto", "none",
                                  "\u00a0zstd\u00a0", ["zstd,zlib"], [""], ["zstd", "unknown"]])
def test_empty_unknown_and_compound_sequence_names_are_value_errors(value: object) -> None:
    for factory in (lambda: configured(algorithms=value), lambda: prepared(value)):
        with pytest.raises(ValueError):
            factory()


class Integer(int):
    def __int__(self) -> int:
        raise AssertionError("exact-level validation must not invoke __int__")

class Level(IntEnum):
    THREE = 3


@pytest.mark.parametrize("value", [None, True, False, 3.0, "3", Integer(3), Level.THREE, ProtocolTrap()])
def test_zstd_level_rejects_nonexact_integers_even_when_zstd_is_absent(value: object) -> None:
    for algorithm in (None, "zlib", "zstd"):
        with pytest.raises(TypeError, match="zstd_compression_level"):
            configured(algorithms=algorithm, zstd_compression_level=value)


@pytest.mark.parametrize("value", [0, -1, 23, 2**100])
def test_zstd_level_range_errors_are_value_errors(value: int) -> None:
    with pytest.raises(ValueError, match="zstd_compression_level"):
        configured(algorithms="zstd", zstd_compression_level=value)


@pytest.mark.parametrize("value", [1, 3, 10, 22])
def test_zstd_level_and_non_zstd_none_are_applicable_native_facts(value: int) -> None:
    zstd = configured(algorithms="zstd", zstd_compression_level=value)
    zlib = configured(algorithms="zlib", zstd_compression_level=value)
    raw = configured(algorithms=None, zstd_compression_level=value)
    assert zstd.zstd_compression_level == value
    assert zlib.zstd_compression_level is None and raw.zstd_compression_level is None
    assert prepared(zstd) == (("zstd",), value)
    assert prepared(zlib) == (("zlib",), value)
    assert prepared(raw) == (("uncompressed",), value)
    assert repr(zstd) == f"Compression(algorithms=('zstd',), zstd_compression_level={value})"
    assert repr(zlib) == "Compression(algorithms=('zlib',))"
    assert repr(raw) == "Compression(algorithms=('uncompressed',))"
    assert str(zstd) == repr(zstd) and str(zlib) == repr(zlib)


def test_input_mutation_cannot_change_the_frozen_native_configuration() -> None:
    source = ["zstd", "uncompressed"]
    config = configured(algorithms=source, zstd_compression_level=10)
    source[:] = ["zlib"]
    assert config.algorithms == ("zstd", "uncompressed") and prepared(config) == (("zstd", "uncompressed"), 10)
    for name, value in (("algorithms", ("zlib",)), ("zstd_compression_level", 1), ("extra", True)):
        with pytest.raises(AttributeError):
            setattr(config, name, value)


class Text(str):
    def __str__(self) -> str:
        raise AssertionError("configuration reads base text")
    def strip(self, _chars: str | None = None) -> str:
        raise AssertionError("configuration trims native ASCII whitespace")
    def lower(self) -> str:
        raise AssertionError("configuration ignores case natively")

class Names(list[str]):
    def __iter__(self) -> Any:
        raise AssertionError("configuration reads base finite sequence")
    def __getitem__(self, _index: Any) -> str:
        raise AssertionError("configuration reads base finite sequence")


def test_configuration_reads_base_finite_text_and_sequence_without_overrides() -> None:
    assert prepared(Text(" ZsTd,zlib ")) == (("zlib", "zstd"), 3)
    assert prepared(Names([Text("ZSTD"), "uncompressed"])) == (("zstd", "uncompressed"), 3)


def test_large_duplicate_sources_and_boundary_whitespace_keep_only_permissions() -> None:
    csv = (" ZsTd ," * 50_000) + "\tZLIB\r\n"
    assert prepared(csv) == (("zlib", "zstd"), 3)
    assert prepared(["zstd"] * 50_000 + ["uncompressed"]) == (("zstd", "uncompressed"), 3)
    assert prepared(" " * 300_000 + "zlib" + "\t" * 300_000) == (("zlib",), 3)


def test_diagnostics_locate_the_same_invalid_native_algorithm_boundary() -> None:
    with pytest.raises(TypeError, match=r"compression\[1\]"):
        prepared(["zstd", None])
    with pytest.raises(ValueError, match=r"algorithms\[2\]"):
        configured(algorithms=["zstd", "zlib", "invalid"])
