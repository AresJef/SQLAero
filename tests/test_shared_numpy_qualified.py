"""Qualified ndarray base storage bypasses replacement Python accessors."""

import gc
import json
import pathlib
import subprocess
import sys

import pytest
from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize("dtype", ["i8", "f4", "U8", "O"])
@pytest.mark.parametrize("shape", [(), (3,), (2, 3)])
def test_real_array_subclass_uses_native_metadata_buffer_and_actual_getitem(dtype: str, shape: tuple[int, ...]) -> None:
    class Child(np.ndarray):
        pass
    size = 1 if not shape else int(np.prod(shape))
    original = np.arange(size).astype(dtype).reshape(shape)
    expected = original.tolist()
    source = original.view(Child)
    sql = _sqlaero._encode_value(original).to_bytes()
    calls: list[str] = []
    def forbidden(*_args: object, **_kwargs: object) -> object:
        calls.append("override")
        raise AssertionError("must read supported ndarray base storage")
    for name in ["__getitem__", "__iter__", "__buffer__", "__release_buffer__", "__array__", "__array_finalize__", "view", "item", "tolist"]:
        setattr(Child, name, forbidden)
    for name in ["dtype", "shape", "strides", "ndim", "base", "flags", "data"]:
        setattr(Child, name, property(forbidden))
    Child.__getattribute__ = forbidden
    assert json.loads(json_dumps(source)) == expected
    assert _sqlaero._encode_value(source).to_bytes() == sql
    assert calls == []


def test_matrix_uses_its_actual_two_axes_without_matrix_getitem_rules() -> None:
    source = np.matrix([[7, 8], [9, 10]], dtype="i8")
    assert json_dumps(source) == b"[[7,8],[9,10]]"
    assert _sqlaero._escape_projection(source) == [("7", "8"), ("9", "10")]


@pytest.mark.parametrize("layout", ["vector", "reverse", "matrix", "transpose"])
def test_existing_memmap_reuses_the_public_physical_owner(tmp_path: pathlib.Path, layout: str) -> None:
    path = tmp_path / "input.bin"
    np.arange(6, dtype="i8").tofile(path)
    original = np.memmap(path, dtype="i8", mode="r", shape=(2, 3))
    source = original[0] if layout == "vector" else original[0, ::-1] if layout == "reverse" else original.T if layout == "transpose" else original
    expected = source.tolist()
    assert json.loads(json_dumps(source)) == expected
    assert _sqlaero._encode_value(source).to_bytes() == _sqlaero._encode_value(np.array(expected, dtype="i8")).to_bytes()
    assert original[0, 0] == 0 and path.read_bytes() == np.arange(6, dtype="i8").tobytes()
    del source, original
    gc.collect()


def test_closed_public_mmap_owner_is_rejected_before_payload_read(tmp_path: pathlib.Path) -> None:
    path = tmp_path / "closed.bin"
    path.write_bytes(b"\0" * 16)
    script = """
import mmap, pathlib, sys
import numpy as np
from sqlaero import _sqlaero, json_dumps
with open(sys.argv[1], 'r+b') as file:
    mapping = mmap.mmap(file.fileno(), 0)
    source = np.ndarray((2,), dtype='i8', buffer=mapping)
    mapping.close()
    for convert in [_sqlaero._encode_value, json_dumps]:
        try:
            convert(source)
        except (ValueError, BufferError):
            pass
        else:
            raise AssertionError('closed allocation owner was read')
"""
    result = subprocess.run([sys.executable, "-c", script, str(path)],
                            capture_output=True, text=True, timeout=15, check=False)
    assert result.returncode == 0, result.stdout + result.stderr


def test_unknown_physical_dtype_is_not_admitted_only_by_kind_and_width() -> None:
    # longdouble may have the same byte width as float64 on a platform; it is
    # still a distinct, explicitly excluded source schema under NP05=B.
    for dtype in [np.longdouble, np.clongdouble]:
        for shape in [(0,), (1,)]:
            value = np.zeros(shape, dtype=dtype)
            for convert in [_sqlaero._encode_value, json_dumps]:
                with pytest.raises(TypeError):
                    convert(value)


@pytest.mark.parametrize("text", ["\ud800", "a\udfffé", "😀\ud800", "\ud800\udc00"])
@pytest.mark.parametrize("dtype", ["U16", ">U16"])
def test_native_unicode_error_payload_is_an_exact_canonical_python_string(text: str, dtype: str) -> None:
    source = np.full(64, "ok", dtype=dtype)
    source[37] = text
    for convert in [json_dumps, _sqlaero._encode_value]:
        with pytest.raises(UnicodeEncodeError) as caught:
            convert(source)
        payload = caught.value.object
        assert type(payload) is str and payload == text
        assert hash(payload) == hash(text)
        assert [ord(character) for character in payload] == [ord(character) for character in text]
