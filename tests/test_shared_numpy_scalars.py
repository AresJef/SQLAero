"""Qualified NumPy scalar subclasses use canonical native storage and releases."""

# pyright: reportPrivateUsage=false


import pytest
from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")


@pytest.mark.parametrize(("base", "number", "sql", "json"), [
    (np.int8, -128, b"-128", b"-128"),
    (np.int16, -32768, b"-32768", b"-32768"),
    (np.int32, -(1 << 31), b"-2147483648", b"-2147483648"),
    (np.int64, -(1 << 63), b"-9223372036854775808", b"-9223372036854775808"),
    (np.uint8, 255, b"255", b"255"),
    (np.uint16, 65535, b"65535", b"65535"),
    (np.uint32, (1 << 32) - 1, b"4294967295", b"4294967295"),
    (np.uint64, (1 << 64) - 1, b"18446744073709551615", b"18446744073709551615"),
    (np.longlong, -17, b"-17", b"-17"),
    (np.ulonglong, 17, b"17", b"17"),
    (np.float16, -0.0, b"-0.0e0", b"-0.0"),
    (np.float32, 0.1, b"0.10000000149011612e0", b"0.10000000149011612"),
    (np.float64, 0.5, b"0.5e0", b"0.5"),
])
def test_numpy_real_subclasses_ignore_replaced_conversion_metadata_and_buffer(
    base: type, number: int | float, sql: bytes, json: bytes,
) -> None:
    calls: list[str] = []
    def forbidden(*_args: object, **_kwargs: object) -> object:
        calls.append("override")
        raise AssertionError("canonical native storage must bypass this override")
    attrs: dict[str, object] = {name: forbidden for name in
             ("__int__", "__index__", "__float__", "__str__", "item", "__array__", "__buffer__", "__release_buffer__")}
    attrs["dtype"] = property(forbidden)
    Child = type("ActualScalar", (base,), attrs)
    value = Child(number)
    assert type(value) is Child
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value) == json
    assert json_dumps([value, value]) == b"[" + json + b"," + json + b"]"
    assert calls == []


@pytest.mark.parametrize("base", [np.float16, np.float32, np.float64])
@pytest.mark.parametrize("number", [float("nan"), float("inf"), -float("inf")])
def test_numpy_subclass_nonfinite_values_share_the_existing_policies(base: type, number: float) -> None:
    def forbidden(self: object) -> float:
        raise AssertionError("must read actual floating base storage")
    Child = type("Child", (base,), {"__float__": forbidden})
    value = Child(number)
    assert _sqlaero._encode_value(value).to_bytes() == b"NULL"
    assert json_dumps(value) == b"null"
    with pytest.raises(ValueError, match="allow_nan"):
        json_dumps(value, allow_nan=False)


def test_fake_numpy_class_does_not_open_an_arbitrary_buffer_protocol() -> None:
    def claimed_class(self: object) -> type:
        return np.int64
    def forbidden(self: object, _flags: int) -> memoryview:
        raise AssertionError("a fake class must not enter a source protocol")
    Fake = type("Fake", (), {"__class__": property(claimed_class), "__buffer__": forbidden})
    for convert in (_sqlaero._encode_value, json_dumps):
        with pytest.raises(TypeError):
            convert(Fake())


def test_qualified_scalar_export_releases_its_source_exactly_once() -> None:
    import gc
    released: list[str] = []
    class Child(np.int64):
        def __del__(self) -> None:
            released.append("source")
        def __buffer__(self, _flags: int) -> memoryview:
            raise AssertionError("subclass getbuffer must not run")
        def __release_buffer__(self, _view: memoryview) -> None:
            raise AssertionError("subclass releasebuffer must not run")
    value = Child(19)
    for _ in range(128):
        assert _sqlaero._encode_value(value).to_bytes() == b"19"
        assert json_dumps(value) == b"19"
    assert released == []
    del value
    gc.collect()
    assert released == ["source"]


def test_qualified_scalar_storage_can_be_read_concurrently_without_overrides() -> None:
    from concurrent.futures import ThreadPoolExecutor
    class Child(np.float32):
        def __float__(self) -> float:
            raise AssertionError("must read shared immutable base storage")
        def __buffer__(self, _flags: int) -> memoryview:
            raise AssertionError("must use the qualified base buffer")
    value = Child(0.5)
    def read(_thread: int) -> None:
        for _ in range(200):
            assert _sqlaero._encode_value(value).to_bytes() == b"0.5e0"
            assert json_dumps([value, value]) == b"[0.5,0.5]"
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(read, range(4)))
