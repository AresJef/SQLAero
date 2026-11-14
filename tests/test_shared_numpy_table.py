"""Independent scalar fixtures for approved NumPy type-table boundaries."""

import json
import math

import pytest

from sqlaero import _sqlaero, json_dumps

np = pytest.importorskip("numpy")

INTEGERS = [
    pytest.param(np.int8, -128, 127, id="024"),
    pytest.param(np.int16, -32768, 32767, id="025"),
    pytest.param(np.int32, -2147483648, 2147483647, id="026"),
    pytest.param(np.int64, -9223372036854775808, 9223372036854775807, id="027"),
    pytest.param(np.longlong, -9223372036854775808, 9223372036854775807, id="028"),
    pytest.param(np.uint8, 0, 255, id="029"),
    pytest.param(np.uint16, 0, 65535, id="030"),
    pytest.param(np.uint32, 0, 4294967295, id="031"),
    pytest.param(np.uint64, 0, 18446744073709551615, id="032"),
    pytest.param(np.ulonglong, 0, 18446744073709551615, id="033"),
]


@pytest.mark.parametrize(("base", "minimum", "maximum"), INTEGERS)
def test_numpy_integer_table_values_boundaries_and_keys(base, minimum, maximum) -> None:
    for number in (7, minimum, maximum):
        source = base(number)
        expected = str(number)
        assert _sqlaero._escape_projection(source) == expected
        assert json_dumps(source) == expected.encode("ascii")
    assert _sqlaero._escape_projection({base(7): "ab"}) == ("'ab'",)
    assert json_dumps({base(7): "ab"}) == b'{"7":"ab"}'
    with pytest.raises(ValueError):
        json_dumps({base(7): "ab", "7": "cd"})


@pytest.mark.parametrize(("base", "minimum", "maximum"), INTEGERS)
def test_numpy_integer_table_source_overflow_is_not_library_validation(base, minimum, maximum, record_property) -> None:
    record_property("verification_boundary", "provider-construction-only")
    record_property("provider_versions", "numpy=" + np.__version__)
    # The two unsigned64 rows explicitly use -2; narrower unsigned rows use -1.
    lower = -2 if minimum == 0 and maximum == 18446744073709551615 else minimum - 1
    for number in (lower, maximum + 1):
        with pytest.raises(OverflowError):
            base(number)


@pytest.mark.parametrize(("base", "minimum", "maximum"), INTEGERS)
def test_numpy_integer_table_true_subclass_uses_base_value(base, minimum, maximum) -> None:
    def forbidden(*args, **kwargs):
        raise AssertionError("real NumPy integer must retain its actual base value")
    Child = type("StoredInteger", (base,), dict(__int__=forbidden, __index__=forbidden,
                                               __str__=forbidden, item=forbidden))
    source = Child(7)
    assert type(source) is Child
    assert _sqlaero._escape_projection(source) == "7"
    assert json_dumps(source) == b"7"


@pytest.mark.parametrize(("number", "sql", "document"), [(True, "1", b"true"), (False, "0", b"false")])
def test_numpy_boolean_table_values_and_key_role(number, sql, document) -> None:
    source = np.bool_(number)
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document
    assert _sqlaero._escape_projection({source: "ab"}) == ("'ab'",)
    with pytest.raises(TypeError):
        json_dumps({source: "ab"})


def test_numpy_boolean_table_conditional_subclass_source(record_property) -> None:
    try:
        Child = type("StoredBoolean", (np.bool_,), {})
    except TypeError as error:
        record_property("verification_boundary", "provider-construction-only")
        record_property("provider_versions", "numpy=" + np.__version__)
        record_property("conditional_subclass_unavailable", str(error))
        return
    source = Child(True)
    if type(source) is not Child:
        record_property("verification_boundary", "provider-construction-only")
        record_property("conditional_subclass_unavailable", "provider constructor did not return a real subclass instance")
        return
    assert _sqlaero._escape_projection(source) == "1"
    assert json_dumps(source) == b"true"


@pytest.mark.parametrize("base", [np.float16, np.float32, np.float64], ids=["034","035","036"])
@pytest.mark.parametrize("number", [2.5, 0.1, -0.0])
def test_numpy_float_table_preserves_the_promoted_value_and_category(base, number) -> None:
    source = base(number)
    # NumPy's public scalar conversion independently provides the exact promoted value.
    expected = float(source)
    sql = _sqlaero._escape_projection(source)
    assert "e" in sql.lower() and float(sql).hex() == expected.hex()
    actual = json.loads(json_dumps(source))
    assert type(actual) is float and actual.hex() == expected.hex()
    assert _sqlaero._escape_projection({base(1): 7}) == ("7",)
    with pytest.raises(TypeError):
        json_dumps({base(1): 7})


@pytest.mark.parametrize("base", [np.float16, np.float32, np.float64], ids=["034","035","036"])
@pytest.mark.parametrize("number", [math.nan, math.inf, -math.inf])
def test_numpy_float_table_nonfinite_policy_does_not_change_sql(base, number) -> None:
    source = base(number)
    assert _sqlaero._escape_projection(source) == "NULL"
    assert json_dumps(source) == b"null"
    with pytest.raises(ValueError):
        json_dumps(source, allow_nan=False)
    assert _sqlaero._escape_projection(source) == "NULL"


@pytest.mark.parametrize(("unsigned", "floating", "bits", "expected_hex"), [
    ("u2","f2",0x7bff,"0x1.ffcp+15"),
    ("u2","f2",0x0400,"0x1p-14"),
    ("u2","f2",0x0001,"0x1p-24"),
    ("u2","f2",0x3c01,"0x1.004p0"),
    ("u4","f4",0x7f7fffff,"0x1.fffffep+127"),
    ("u4","f4",0x00800000,"0x1p-126"),
    ("u4","f4",0x00000001,"0x1p-149"),
    ("u4","f4",0x3f800001,"0x1.000002p0"),
], ids=["034-08","034-09","034-10","034-11","035-08","035-09","035-10","035-11"])
def test_numpy_float_table_fixed_bits_do_not_flush_or_use_a_short_source_string(unsigned, floating, bits, expected_hex) -> None:
    source = np.array([bits], dtype=unsigned).view(floating)[0]
    expected = float.fromhex(expected_hex)
    assert float(_sqlaero._escape_projection(source)).hex() == expected.hex()
    actual = json.loads(json_dumps(source))
    assert type(actual) is float and actual.hex() == expected.hex()
    assert math.copysign(1, actual) == math.copysign(1, expected)


@pytest.mark.parametrize(("text", "sql", "document"), [
    ("ab", "'ab'", b'"ab"'), ("", "''", b'""'), ("a\0b", "'a\\0b'", b'"a\\u0000b"'),
], ids=["037-01","037-02","037-03"])
def test_numpy_text_table_keeps_the_complete_real_unicode(text, sql, document) -> None:
    source = np.str_(text)
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document


def test_numpy_text_table_rejects_real_surrogate_codepoints() -> None:
    for text in ("\ud800", "\ud83d\ude00"):
        source = np.str_(text)
        for convert in (_sqlaero._escape_projection, json_dumps):
            with pytest.raises(UnicodeEncodeError):
                convert(source)


@pytest.mark.parametrize(("value", "expected"), [
    (b"ab", "CAST(X'6162' AS BINARY)"), (b"", "CAST(X'' AS BINARY)"),
    (b"a\0b\0", "CAST(X'61006200' AS BINARY)"),
], ids=["038-01","038-02","038-03"])
def test_numpy_binary_table_uses_actual_source_length_without_guessing_json(value, expected) -> None:
    source = np.bytes_(value)
    assert _sqlaero._escape_projection(source) == expected
    with pytest.raises(TypeError):
        json_dumps(source)


@pytest.mark.parametrize(("base", "value", "sql", "document"), [
    (np.str_, "ab", "'ab'", b'"ab"'), (np.bytes_, b"ab", "CAST(X'6162' AS BINARY)", None),
], ids=["037-05","038-04"])
def test_numpy_text_binary_table_real_subclasses_ignore_replacements(base, value, sql, document) -> None:
    def forbidden(*args, **kwargs):
        raise AssertionError("use actual text/binary base storage")
    Child = type("StoredText", (base,), dict(__str__=forbidden, __bytes__=forbidden))
    source = Child(value)
    assert _sqlaero._escape_projection(source) == sql
    if document is None:
        with pytest.raises(TypeError):
            json_dumps(source)
    else:
        assert json_dumps(source) == document


@pytest.mark.parametrize(("source", "sql", "document"), [
    pytest.param(np.datetime64("2026-11-12","D"), "'2026-11-12 00:00:00'", b'"2026-11-12T00:00:00"', id="039-04"),
    pytest.param(np.datetime64("2026-11-12T08","h"), "'2026-11-12 08:00:00'", b'"2026-11-12T08:00:00"', id="039-05"),
    pytest.param(np.datetime64("2026-11-12T08:09","m"), "'2026-11-12 08:09:00'", b'"2026-11-12T08:09:00"', id="039-06"),
    pytest.param(np.datetime64("2026-11-12T08:09:10","s"), "'2026-11-12 08:09:10'", b'"2026-11-12T08:09:10"', id="039-07"),
    pytest.param(np.datetime64("2026-11-12T08:09:10.123456000","ns"), "'2026-11-12 08:09:10.123456'", b'"2026-11-12T08:09:10.123456"', id="039-10"),
    pytest.param(np.array(1,dtype="datetime64[2D]")[()], "'1970-01-03 00:00:00'", b'"1970-01-03T00:00:00"', id="039-13"),
    pytest.param(np.datetime64("NaT","ns"), "NULL", b"null", id="039-17"),
    pytest.param(np.datetime64("2026-11-12T08:09:10.000000009","ns"), "'2026-11-12 08:09:10'", b'"2026-11-12T08:09:10.000000009"', id="039-20"),
    pytest.param(np.datetime64("2026-11-12T08:09:10.123456900","ns"), "'2026-11-12 08:09:10.123456'", b'"2026-11-12T08:09:10.123456900"', id="039-21"),
    pytest.param(np.timedelta64(90,"s"), "'00:01:30'", b'"PT90S"', id="040-01"),
    pytest.param(np.timedelta64(1,"us"), "'00:00:00.000001'", b'"PT0.000001S"', id="040-03"),
    pytest.param(np.array(3,dtype="timedelta64[2D]")[()], "'144:00:00'", b'"P6D"', id="040-08"),
    pytest.param(np.timedelta64(14,"M"), "'10226:47:24'", b'"P14M"', id="040-11"),
    pytest.param(np.timedelta64(0,"s"), "'00:00:00'", b'"P0D"', id="040-13"),
    pytest.param(np.timedelta64(0,"Y"), "'00:00:00'", b'"P0Y"', id="040-15"),
    pytest.param(np.timedelta64("NaT","ns"), "NULL", b"null", id="040-16"),
    pytest.param(np.timedelta64(999,"ns"), "'00:00:00'", b'"PT0.000000999S"', id="040-18"),
    pytest.param(np.timedelta64(-999,"ns"), "'00:00:00'", b'"-PT0.000000999S"', id="040-19"),
    pytest.param(np.timedelta64(1000,"ns"), "'00:00:00.000001'", b'"PT0.000001S"', id="040-20"),
    pytest.param(np.timedelta64(-1001,"ns"), "'-00:00:00.000001'", b'"-PT0.000001001S"', id="040-21"),
],)
def test_numpy_temporal_table_remaining_exact_policy_pairs(source, sql, document) -> None:
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document


@pytest.mark.parametrize(("base", "value", "sql", "document"), [
    (np.datetime64, np.datetime64("2026-11-12T08:09:10.123456789","ns"),
     "'2026-11-12 08:09:10.123456'", b'"2026-11-12T08:09:10.123456789"'),
    (np.timedelta64, np.timedelta64(-1,"ns"), "'00:00:00'", b'"-PT0.000000001S"'),
], ids=["039-23","040-22"])
def test_numpy_temporal_table_conditional_subclass_source(base, value, sql, document, record_property) -> None:
    def forbidden(*args, **kwargs):
        raise AssertionError("a real temporal subclass cannot replace its value")
    Child = type("StoredTemporal", (base,), dict(__str__=forbidden, item=forbidden))
    source = Child(value)
    if type(source) is not Child:
        record_property("verification_boundary", "provider-construction-only")
        record_property("provider_versions", "numpy=" + np.__version__)
        record_property("conditional_subclass_unavailable", "provider constructor returns the canonical base scalar")
        return
    assert _sqlaero._escape_projection(source) == sql
    assert json_dumps(source) == document


@pytest.mark.parametrize("base", [np.datetime64, np.timedelta64], ids=["039-17","040-16"])
def test_numpy_temporal_table_real_nat_is_valid_in_strict_json(base) -> None:
    source = base("NaT", "ns")
    assert _sqlaero._escape_projection(source) == "NULL"
    assert json_dumps(source, allow_nan=False) == b"null"
