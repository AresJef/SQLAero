"""Immutable metadata, naming rules, and lazy cache contracts."""

# pyright: reportPrivateUsage=false

from collections.abc import Sequence
import pytest
from sqlaero import ColumnInfo, Columns
from sqlaero._sqlaero import _Rows, _rows_from_packets
from test_buffered_reader import length, packets


def field(
    name: bytes = b"id", *, table: bytes = b"u", database: bytes = b"db",
    original_name: bytes = b"id", original_table: bytes = b"users",
    type_code: int = 3, collation_id: int = 63, flags: int = 0,
    column_length: int = 11, decimals: int = 0,
) -> bytes:
    labels = (b"def", database, table, original_table, name, original_name)
    return (
        b"".join(length(value) for value in labels) + b"\x0c"
        + collation_id.to_bytes(2, "little") + column_length.to_bytes(4, "little")
        + bytes([type_code]) + flags.to_bytes(2, "little") + bytes([decimals, 0, 0])
    )


def result(columns: Sequence[bytes], *, strict: bool = True) -> _Rows:
    return _rows_from_packets(packets(list(columns), []), decode_text_strict=strict)


@pytest.mark.parametrize(
    ("labels", "tables", "expected"),
    [
        (["id", "id"], ["u", "u"], ("id.1", "id.2")),
        (["id", "id"], ["u", "o"], ("u.id", "o.id")),
        (["id", "id", "id"], ["u", "o", "o"], ("u.id", "o.id.1", "o.id.2")),
        (["id", "id"], ["u", ""], ("u.id", "id")),
        (["id", "id", "id"], ["u", "u", ""], ("u.id.1", "u.id.2", "id")),
        (["x", "x"], ["", ""], ("x.1", "x.2")),
        (["id", "id", "id.1"], ["u", "u", ""], ("id.2", "id.3", "id.1")),
        (["id", "id", "u.id"], ["u", "o", ""], ("u.id.1", "o.id", "u.id")),
    ],
)
def test_native_unique_keys_and_exact_lookup(
    labels: list[str], tables: list[str], expected: tuple[str, ...],
) -> None:
    columns = result([
        field(label.encode(), table=table.encode())
        for label, table in zip(labels, tables, strict=True)
    ]).columns
    assert columns.names == tuple(labels)
    assert columns.keys == expected
    assert columns.names is columns.names
    assert columns.keys is columns.keys
    for index, key in enumerate(expected):
        assert columns[key] is columns[index]
        assert columns[index].key is columns.keys[index]
        assert columns[index].index == index
    with pytest.raises(KeyError):
        columns["missing"]


def test_item_slice_iteration_and_snapshot_lifetime() -> None:
    value = result([field(b"a"), field(b"b"), field(b"c")])
    columns = value.columns
    assert columns is value.columns
    assert isinstance(columns, Columns)
    assert isinstance(columns[0], ColumnInfo)
    assert tuple(columns) == columns[:]
    assert columns[-1] is columns[2]
    assert columns[::-1] == (columns[2], columns[1], columns[0])
    assert columns[99:] == ()
    for index in (3, -4, 10**10000):
        with pytest.raises(IndexError):
            columns[index]
    with pytest.raises(ValueError):
        columns[::0]
    with pytest.raises(TypeError):
        columns[1.5]  # type: ignore[index]
    saved = columns[0]
    description = value.description
    assert description is value.description
    value.close()
    del value
    assert columns.names == ("a", "b", "c")
    assert saved.name == "a"
    assert saved.name is saved.name
    assert description is not None and description[0] == ("a", 3, None, None, None, 0, True)
    with pytest.raises(AttributeError):
        setattr(saved, "name", "changed")
    with pytest.raises(AttributeError):
        setattr(columns, "names", ())


def test_key_preparation_is_not_required_for_position_or_name_access() -> None:
    value = result([field(b"first", table=b"\xff"), field(b"second")])
    columns = value.columns
    assert columns.names == ("first", "second")
    assert columns[0].name == "first"
    assert columns[:1] == (columns[0],)
    assert value.description is not None
    with pytest.raises(UnicodeDecodeError):
        _ = columns.keys
    assert columns[0].name == "first"


def test_surrogateescaped_names_preserve_exact_key_lookup() -> None:
    columns = result([field(b"bad\xff", table=b"")], strict=False).columns
    assert columns.names == ("bad\udcff",)
    assert columns.keys == columns.names
    assert columns["bad\udcff"] is columns[0]


@pytest.mark.parametrize('workers', [2, 4])
@pytest.mark.parametrize('width', [256, 512])
@pytest.mark.parametrize('closed', [False, True])
@pytest.mark.parametrize('duplicate', [False, True])
@pytest.mark.parametrize('entry', ['keys', 'column-key', 'lookup'])
def test_concurrent_cold_key_readers_reuse_one_complete_publication(
    workers: int, width: int, closed: bool, duplicate: bool, entry: str,
) -> None:
    from concurrent.futures import ThreadPoolExecutor
    from threading import Barrier

    names = [b'id'] * width if duplicate else [f'c{index}'.encode() for index in range(width)]
    first = 'id.1' if duplicate else 'c0'
    source = result([field(name) for name in names])
    columns = source.columns
    if closed:
        source.close()
    ready = Barrier(workers)

    def read(_index: int) -> object:
        ready.wait(timeout=5)
        if entry == 'keys':
            return columns.keys
        if entry == 'column-key':
            return columns[0].key
        return columns[first]

    with ThreadPoolExecutor(max_workers=workers) as executor:
        observed = list(executor.map(read, range(workers)))
    expected = tuple(f'id.{index + 1}' for index in range(width)) if duplicate else tuple(
        name.decode() for name in names)
    assert columns.keys == expected
    assert all(value is observed[0] for value in observed)
    if entry == 'keys':
        assert observed[0] is columns.keys
    elif entry == 'column-key':
        assert observed[0] is columns[0].key and observed[0] == first
    else:
        assert observed[0] is columns[0]
    assert source.closed is closed


def test_optional_flags_types_numeric_facts_and_description() -> None:
    value = result([
        field(b"price", type_code=246, flags=1 | 32 | 64 | 512 | 2 | 4 | 8,
              column_length=11, decimals=2),
        field(b"moment", type_code=12, decimals=6),
        field(b"binary", type_code=253),
        field(b"text", type_code=253, collation_id=46),
        field(b"enum", type_code=254, flags=256),
        field(b"set", type_code=254, flags=2048),
        field(b"future", type_code=241, collation_id=65000),
        field(b"expression", table=b"", database=b"", original_name=b"", original_table=b""),
    ])
    columns = value.columns
    price = columns[0]
    assert (price.precision, price.scale, price.is_unsigned, price.is_zerofill) == (10, 2, True, True)
    assert price.type_name == "NEWDECIMAL"
    assert price.catalog == "def"
    assert price.original_name == "id" and price.original_table == "users"
    assert price.table == "u" and price.database == "db"
    assert not price.is_nullable
    assert price.is_auto_increment and price.is_primary_key_part
    assert price.is_unique_key_part and price.is_non_unique_key_part
    assert price.column_length == 11 and price.decimals == 2
    assert columns[1].fractional_seconds_precision == 6
    assert columns[2].is_binary_string
    assert not columns[3].is_binary_string
    assert columns[3].charset_name == "utf8mb4"
    assert columns[3].collation_name == "utf8mb4_bin"
    assert columns[4].is_enum and not columns[4].is_binary_string
    assert columns[5].is_set and not columns[5].is_binary_string
    assert columns[6].type_code == 241 and columns[6].type_name is None
    assert columns[6].collation_id == 65000
    assert columns[6].charset_name is None and columns[6].collation_name is None
    assert columns[7].table is None and columns[7].database is None
    assert columns[7].original_name is None and columns[7].original_table is None
    assert value.description is not None
    assert value.description[0] == ("price", 246, None, None, 10, 2, False)
    assert Columns.names.__doc__ and ColumnInfo.precision.__doc__


def test_no_row_result_has_empty_columns_but_zero_rows_keep_real_description() -> None:
    empty = _rows_from_packets([b"\x00\x00\x00\x02\x00\x00\x00"])
    assert empty.description is None
    columns = empty.columns
    assert columns is empty.columns
    assert columns.names == columns.keys == ()
    assert tuple(columns) == () and columns[:] == ()
    with pytest.raises(IndexError):
        columns[0]
    with pytest.raises(KeyError):
        columns[""]
    rows = result([field()])
    assert rows.description is not None
    assert len(rows.columns) == 1
    for cls in (Columns, ColumnInfo):
        with pytest.raises(TypeError):
            cls()  # pyright: ignore[reportCallIssue] -- intentional invalid construction


def test_fixed_seed_access_order_reuses_objects_and_preserves_old_metadata() -> None:
    import random
    seed = 20261026
    generator = random.Random(seed)
    value = result([field(b'id', table=b'u'), field(b'id', table=b'o'), field(b'label', table=b'')])
    columns = value.columns
    names = ('id', 'id', 'label')
    keys = ('u.id', 'o.id', 'label')
    for step in range(64):
        position = generator.randrange(3)
        operation = generator.randrange(5)
        if operation == 0:
            assert columns.names == names, (seed, step, operation)
        elif operation == 1:
            assert columns.keys == keys, (seed, step, operation)
        elif operation == 2:
            assert columns[position] is columns[keys[position]], (seed, step, operation)
        elif operation == 3:
            assert columns[position - 3] is columns[position], (seed, step, operation)
        else:
            assert tuple(item.name for item in columns[position:]) == names[position:], (seed, step, operation)
    retained = columns[0]
    value.close()
    assert columns.keys == keys and retained is columns['u.id'] and retained.name == 'id'
