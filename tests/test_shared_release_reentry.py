"""Releasing acquired metadata may run Python before the next source protocol."""

from collections import namedtuple
from datetime import datetime, timedelta, tzinfo
from uuid import uuid4

import pytest

from sqlaero import JSON, Cursor, DictCursor, SSCursor, SSDictCursor
from sqlaero.errors import InterfaceBusyError, InterfaceError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True])
def test_later_source_protocol_is_not_called_after_metadata_finalizer_retires_cursor(label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool) -> None:
    name = "release_reentry_" + uuid4().hex[:12]
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute(f"CREATE TEMPORARY TABLE `{name}` (stamp DATETIME(6)) ENGINE=InnoDB")
        context = connection.transaction(kind, prepared=prepared)
        current = context.__enter__()
        retired = False
        exits: list[InterfaceBusyError] = []
        later: list[str] = []
        try:
            current.execute(f"INSERT INTO `{name}` VALUES (%s)", (datetime(2024, 1, 1),))
            def finish() -> None:
                nonlocal retired
                try:
                    context.__exit__(None, None, None)
                except InterfaceBusyError as error:
                    exits.append(error)
                finally:
                    retired = True
            class Field(str):
                def __del__(self) -> None:
                    if not retired:
                        finish()
            Point = namedtuple("Point", "stamp")  # pyright: ignore[reportUntypedNamedTuple]  # Exercise collections' real factory.
            Point._fields = (Field("stamp"),)
            class Change(tzinfo):
                def utcoffset(self, dt: datetime | None):
                    # The reader already owns the old immutable metadata. Its
                    # final release, after this callback returns, invokes Field.
                    Point._fields = ("stamp",)
                    return timedelta(0)
            class Later(tzinfo):
                def utcoffset(self, dt: datetime | None):
                    later.append("offset")
                    return timedelta(0)
            value = JSON([Point(datetime(2024, 1, 1, tzinfo=Change())),
                          datetime(2030, 1, 1, tzinfo=Later())])
            caught: Exception | None = None
            try:
                current.execute(f"INSERT INTO `{name}` VALUES (%s)", (value,))
            except Exception as error:
                caught = error
            assert retired and len(exits) == 1 and current.closed and not connection.closed
            with connection.cursor() as check:
                check.execute(f"SELECT COUNT(*) FROM `{name}`")
                assert check.fetchone() == (1,)
            assert not later, "a later Python source protocol ran after metadata released and retired the Cursor"
            assert isinstance(caught, InterfaceError)
            connection.rollback()
            with connection.cursor() as check:
                check.execute(f"SELECT COUNT(*) FROM `{name}`")
                assert check.fetchone() == (0,)
        finally:
            if not retired:
                context.__exit__(RuntimeError, RuntimeError("test cleanup"), None)
