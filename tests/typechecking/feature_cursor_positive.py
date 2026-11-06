"""Both factories preserve the complete row/mode selection matrix."""

from collections.abc import Mapping
from typing import Any, Literal, SupportsIndex, assert_type

from sqlaero import Connection, Cursor, DictCursor, SSCursor, SSDictCursor


def factories(connection: Connection, streaming: bool, optional: bool | None,
              mixed: type[Cursor] | type[SSDictCursor]) -> None:
    assert_type(connection.cursor(), Cursor)
    assert_type(connection.cursor(None, streaming=None), Cursor)
    assert_type(connection.cursor(None, streaming=False), Cursor)
    assert_type(connection.cursor(None, streaming=True), SSCursor)
    assert_type(connection.cursor(tuple, streaming=None), Cursor)
    assert_type(connection.cursor(tuple, streaming=False), Cursor)
    assert_type(connection.cursor(tuple, streaming=True), SSCursor)
    assert_type(connection.cursor(Cursor, streaming=None), Cursor)
    assert_type(connection.cursor(Cursor, streaming=False), Cursor)
    assert_type(connection.cursor(Cursor, streaming=True), SSCursor)
    assert_type(connection.cursor(SSCursor, streaming=None), SSCursor)
    assert_type(connection.cursor(SSCursor, streaming=False), Cursor)
    assert_type(connection.cursor(SSCursor, streaming=True), SSCursor)
    assert_type(connection.cursor(dict, streaming=None), DictCursor)
    assert_type(connection.cursor(dict, streaming=False), DictCursor)
    assert_type(connection.cursor(dict, streaming=True), SSDictCursor)
    assert_type(connection.cursor(DictCursor, streaming=None), DictCursor)
    assert_type(connection.cursor(DictCursor, streaming=False), DictCursor)
    assert_type(connection.cursor(DictCursor, streaming=True), SSDictCursor)
    assert_type(connection.cursor(SSDictCursor, streaming=None), SSDictCursor)
    assert_type(connection.cursor(SSDictCursor, streaming=False), DictCursor)
    assert_type(connection.cursor(SSDictCursor, streaming=True), SSDictCursor)
    assert_type(connection.cursor(Cursor, prepared=True), Cursor)
    assert_type(connection.cursor(SSCursor, prepared=True), SSCursor)
    assert_type(connection.cursor(DictCursor, prepared=True), DictCursor)
    assert_type(connection.cursor(SSDictCursor, prepared=True), SSDictCursor)
    assert_type(connection.cursor(tuple, streaming=streaming), Cursor | SSCursor)
    assert_type(connection.cursor(SSCursor, streaming=optional), Cursor | SSCursor)
    assert_type(connection.cursor(dict, streaming=streaming), DictCursor | SSDictCursor)
    assert_type(connection.cursor(SSDictCursor, streaming=optional), DictCursor | SSDictCursor)
    assert_type(connection.cursor(mixed), Cursor | SSDictCursor)
    assert_type(connection.cursor(mixed, streaming=True), SSCursor | SSDictCursor)
    assert_type(connection.cursor(mixed, streaming=False), Cursor | DictCursor)
    assert_type(connection.cursor(mixed, streaming=optional), Cursor | SSCursor | DictCursor | SSDictCursor)

    with connection.transaction() as a:
        assert_type(a, Cursor)
    with connection.transaction(None, streaming=None) as b:
        assert_type(b, Cursor)
    with connection.transaction(None, streaming=False) as c:
        assert_type(c, Cursor)
    with connection.transaction(None, streaming=True) as d:
        assert_type(d, SSCursor)
    with connection.transaction(tuple, streaming=None) as e:
        assert_type(e, Cursor)
    with connection.transaction(tuple, streaming=False) as f:
        assert_type(f, Cursor)
    with connection.transaction(tuple, streaming=True) as g:
        assert_type(g, SSCursor)
    with connection.transaction(Cursor, streaming=None) as h:
        assert_type(h, Cursor)
    with connection.transaction(Cursor, streaming=False) as i:
        assert_type(i, Cursor)
    with connection.transaction(Cursor, streaming=True) as j:
        assert_type(j, SSCursor)
    with connection.transaction(SSCursor, streaming=None) as k:
        assert_type(k, SSCursor)
    with connection.transaction(SSCursor, streaming=False) as l:
        assert_type(l, Cursor)
    with connection.transaction(SSCursor, streaming=True) as m:
        assert_type(m, SSCursor)
    with connection.transaction(dict, streaming=None) as n:
        assert_type(n, DictCursor)
    with connection.transaction(dict, streaming=False) as o:
        assert_type(o, DictCursor)
    with connection.transaction(dict, streaming=True) as p:
        assert_type(p, SSDictCursor)
    with connection.transaction(DictCursor, streaming=None) as q:
        assert_type(q, DictCursor)
    with connection.transaction(DictCursor, streaming=False) as r:
        assert_type(r, DictCursor)
    with connection.transaction(DictCursor, streaming=True) as s:
        assert_type(s, SSDictCursor)
    with connection.transaction(SSDictCursor, streaming=None) as t:
        assert_type(t, SSDictCursor)
    with connection.transaction(SSDictCursor, streaming=False) as u:
        assert_type(u, DictCursor)
    with connection.transaction(SSDictCursor, streaming=True) as v:
        assert_type(v, SSDictCursor)
    with connection.transaction(dict, streaming=True, prepared=True) as w:
        assert_type(w, SSDictCursor)
    with connection.transaction(mixed, streaming=optional, prepared=True) as x:
        assert_type(x, Cursor | SSCursor | DictCursor | SSDictCursor)


def rows(buffered: Cursor, mapping: DictCursor, stream: SSCursor, mapped_stream: SSDictCursor,
         index: SupportsIndex) -> None:
    assert_type(buffered.execute("SELECT 1"), int)
    assert_type(mapping.execute("SELECT 1"), int)
    assert_type(stream.execute("SELECT 1"), int | None)
    assert_type(mapped_stream.executemany("SELECT %s", [1, 2]), int | None)
    assert_type(buffered.fetchone(), tuple[object, ...] | None)
    assert_type(stream.fetchall(), tuple[tuple[object, ...], ...])
    assert_type(mapping.fetchone(), dict[str, object] | None)
    assert_type(mapping.fetchmany(0), tuple[dict[str, object], ...])
    assert_type(mapped_stream.fetchall(), tuple[dict[str, object], ...])
    assert_type(next(mapping), dict[str, object])
    assert_type(iter(mapping), DictCursor)
    assert_type(iter(mapped_stream), SSDictCursor)
    assert_type(buffered.nextset(), Literal[True] | None)
    assert_type(buffered.scroll(index, mode="absolute"), None)
    assert_type(stream.scroll(0), None)
    assert_type(buffered.mogrify("SELECT %s", [1, 2], many=True), str | list[str])
    assert_type(mapped_stream.mogrify("SELECT %s", None), str | list[str])
    assert_type(stream.batch_row_count, int | None)
    assert_type(buffered.prepared, bool)
    assert_type(buffered.callproc("report", (5, None), database="analytics",
                                  parameter_modes=["IN", "OUT"]), int)
    assert_type(mapping.callproc("report"), int)
    assert_type(stream.callproc("report"), int | None)
    assert_type(mapped_stream.callproc("report"), int | None)
    assert_type(stream.call_output_params, Mapping[int, Any] | None)
    assert_type(mapped_stream.execute("LOAD DATA LOCAL INFILE 'label' INTO TABLE t", local_file="data.csv"), int | None)
