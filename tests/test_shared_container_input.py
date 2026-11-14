"""Real container storage is shared; target layout never calls replacements."""

import pytest
import os
from pathlib import Path
import subprocess
import sys
import textwrap
from concurrent.futures import ThreadPoolExecutor

from sqlaero import _sqlaero, json_dumps


def hostile(base: type, data: object) -> object:
    def forbidden(*_args: object, **_kwargs: object) -> object:
        raise AssertionError("replacement container method must not execute")
    Child = type("Child", (base,), {name: forbidden for name in
        ("__iter__", "__len__", "__getitem__", "__str__", "values", "keys", "items", "copy")})
    return Child(data)


@pytest.mark.parametrize(("base", "data", "sql", "json"), [
    (list, [7, 8], b"(7,8)", b"[7,8]"),
    (tuple, (7, 8), b"(7,8)", b"[7,8]"),
    (dict, {"a": 7, "b": 8}, b"(7,8)", b'{"a":7,"b":8}'),
    (set, {7}, b"(7)", b"[7]"),
    (frozenset, {7}, b"(7)", b"[7]"),
])
def test_true_container_subclasses_use_base_storage_for_both_targets(base: type, data: object, sql: bytes, json: bytes) -> None:
    value = hostile(base, data)
    assert _sqlaero._encode_value(value).to_bytes() == sql
    assert json_dumps(value) == json


@pytest.mark.parametrize("base", [list, tuple, dict])
@pytest.mark.parametrize("many", [False, True])
def test_true_subclass_root_layout_reuses_the_same_actual_elements(base: type, many: bool) -> None:
    source = hostile(base, {"a": 7, "b": 8} if base is dict else [7, 8])
    if many:
        assert _sqlaero._bind_sql("SELECT %s", source, many=True) == [b"\x03SELECT 7", b"\x03SELECT 8"]
    else:
        assert _sqlaero._bind_sql("SELECT %s,%s", source) == [b"\x03SELECT 7,8"]


@pytest.mark.parametrize("base", [list, dict])
def test_cycle_identity_does_not_depend_on_a_normalized_copy(base: type) -> None:
    source = hostile(base, [] if base is list else {})
    if base is list:
        list.append(source, source)
    else:
        dict.__setitem__(source, "self", source)
    with pytest.raises(Exception, match="cycle"):
        _sqlaero._encode_value(source)
    with pytest.raises(ValueError, match="cycle"):
        json_dumps(source)


def test_an_unregistered_iterable_does_not_gain_implicit_support() -> None:
    class Iterable:
        def __iter__(self):
            raise AssertionError("unregistered iterable must not be called")
    for convert in (_sqlaero._encode_value, json_dumps):
        with pytest.raises(TypeError):
            convert(Iterable())


@pytest.mark.parametrize(("projection", "sql", "json"), [
    (dict.keys, b"('a','b')", b'["a","b"]'),
    (dict.values, b"(7,8)", b"[7,8]"),
    (dict.items, b"(7,8)", b'[["a",7],["b",8]]'),
])
def test_dict_views_keep_their_real_projection_without_forwarding_overrides(projection, sql: bytes, json: bytes) -> None:
    value = hostile(dict, {"a": 7, "b": 8})
    view = projection(value)
    assert _sqlaero._encode_value(view).to_bytes() == sql
    assert json_dumps(view) == json


def test_keys_keep_complete_unicode_names_and_reject_normalized_collisions() -> None:
    assert json_dumps({"a\0x": 7, "a\0y": 8}) == b'{"a\\u0000x":7,"a\\u0000y":8}'
    assert json_dumps({"é": 7, "e\u0301": 8}) == '{"é":7,"é":8}'.encode()
    assert json_dumps({"A": 7, "a": 8}) == b'{"A":7,"a":8}'
    for value in ({1: 7, "1": 8}, {-7: 7, "-7": 8}):
        with pytest.raises(ValueError, match="collid"):
            json_dumps(value)


def test_true_key_subclasses_ignore_display_and_conversion_replacements() -> None:
    class Integer(int):
        def __int__(self):
            raise AssertionError("no int conversion")
        def __index__(self):
            raise AssertionError("no index conversion")
        def __str__(self):
            raise AssertionError("no int display conversion")
    class Text(str):
        def __str__(self):
            raise AssertionError("no text display conversion")
    assert json_dumps({Integer(7): 1, Text("a"): 2}) == b'{"7":1,"a":2}'


@pytest.mark.parametrize("key", [True, False, None, 1.5, (1, 2), b"a"])
def test_non_key_families_do_not_gain_string_conversion(key: object) -> None:
    with pytest.raises(TypeError, match="keys"):
        json_dumps({key: 7})


def test_wide_integer_keys_share_the_exact_bounded_conversion() -> None:
    integer = 10**6000 + 123
    text = "1" + "0" * 5997 + "123"
    assert json_dumps({integer: 7}) == b'{"' + text.encode() + b'":7}'
    # Acquisition fragment sizes differ; normalized equality still must agree.
    with pytest.raises(ValueError, match="collid"):
        json_dumps({integer: 7, text: 8})


def test_long_key_compares_and_escapes_full_suffixes() -> None:
    prefix = "😀" * 100000
    assert json_dumps({prefix + "x": 7, prefix + "y": 8}) == (
        b'{"' + prefix.encode() + b'x":7,"' + prefix.encode() + b'y":8}'
    )


def test_shared_stable_subclass_containers_have_independent_concurrent_outputs() -> None:
    source = hostile(dict, {"first": hostile(list, [7, 8]), "second": {7}})
    with ThreadPoolExecutor(max_workers=4) as pool:
        outputs = list(pool.map(lambda _index: json_dumps(source), range(64)))
    assert outputs == [b'{"first":[7,8],"second":[7]}'] * 64


def test_long_key_work_returns_to_signal_checks_without_publishing_a_prefix() -> None:
    script = textwrap.dedent("""
        import sys
        from _native_signal import NativeAlarm, native_call_offsets
        from sqlaero import json_dumps
        source = {"é" * 3_000_000: 7}
        original = MemoryError('original long-key signal')
        count = 0
        def serialize():
            return json_dumps(source)
        offsets = native_call_offsets(serialize.__code__)
        def interrupt(number, frame):
            global count
            if frame is None or frame.f_code is not serialize.__code__ or frame.f_lasti not in offsets:
                return
            count += 1
            if count == 3:
                alarm.cancel()
                raise original
        alarm = NativeAlarm(interrupt)
        try:
            alarm.start(0.001, 0.001, wait_ready=True)
            try:
                serialize()
            except MemoryError as error:
                assert error is original and count == 3
                assert 'while serializing JSON at data[key#0]' in '\\n'.join(error.__notes__)
            else:
                raise AssertionError('long key had no growing-work checks')
        finally:
            alarm.close()
    """)
    env = dict(os.environ)
    env["PYTHONPATH"] = str(Path(__file__).parent) + os.pathsep + env.get("PYTHONPATH", "")
    result = subprocess.run([sys.executable, "-c", script], env=env, capture_output=True, text=True, timeout=15)
    assert result.returncode == 0, (result.stdout, result.stderr)
