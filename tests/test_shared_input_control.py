"""Source protocols revalidate actual owners without inventing execution facts."""

# pyright: reportPrivateUsage=false


from datetime import datetime, timedelta, tzinfo
from datetime import time as clock_time
import time
from typing import Any
from types import FrameType
import re
from uuid import uuid4

import pytest
from sqlaero import Cursor, DictCursor, SSCursor, SSDictCursor, JSON
from sqlaero import _sqlaero
from sqlaero.errors import InterfaceBusyError, InterfaceError, SQLAeroErrorCode
from test_public_connection import live, requires_mysql
from _native_signal import NativeAlarm


@pytest.mark.parametrize("kind", ["text", "bytes", "range"])
def test_medium_values_charge_cumulative_work_before_final_publication(kind: str) -> None:
    leaf: str | bytes = "a" * 32768
    if kind == "bytes":
        # Hex is twice the payload; keep the complete request below 64 MiB.
        leaf = b"a" * 16384
    source = range(4_000_000) if kind == "range" else [leaf] * 1024
    original = RuntimeError("original signal during cumulative value work")
    deliveries = 0

    def interrupt(number: int, frame: FrameType | None) -> None:
        nonlocal deliveries
        if frame is None or frame.f_code is not test_medium_values_charge_cumulative_work_before_final_publication.__code__:
            return
        deliveries += 1
        if deliveries < 3:
            return
        alarm.cancel()
        raise original

    class Zone(tzinfo):
        def utcoffset(self, dt: datetime | None) -> timedelta:
            alarm.start(0.0001, 0.0001, wait_ready=True)
            return timedelta(0)

    alarm = NativeAlarm(interrupt)
    try:
        with pytest.raises(RuntimeError) as caught:
            _sqlaero._bind_sql("SELECT %s,%s", (datetime(2024, 1, 1, tzinfo=Zone()), source))
        assert caught.value is original and deliveries == 3
        notes = "\n".join(getattr(original, "__notes__", ()))
        assert re.search(r"args\[1\]\[\d+\]", notes), notes
        assert "current request was not sent" in notes
    finally:
        alarm.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("kind", [Cursor, DictCursor, SSCursor, SSDictCursor])
@pytest.mark.parametrize("prepared", [False, True], ids=["text", "prepared"])
@pytest.mark.parametrize("entry", ["execute", "callproc"])
@pytest.mark.parametrize("source_kind", ["datetime", "struct-time", "json-dict", "json-time"])
@pytest.mark.parametrize("raises", [False, True], ids=["returns", "raises"])
def test_source_callback_cannot_execute_after_transaction_retires_its_cursor(
    label: str, kind: type[Cursor] | type[DictCursor] | type[SSCursor] | type[SSDictCursor], prepared: bool, entry: str, source_kind: str, raises: bool,
) -> None:
    name = "shared_control_" + uuid4().hex[:16]
    procedure = name + "_p"
    original = MemoryError("original source callback after retirement")
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute(f"CREATE TEMPORARY TABLE `{name}` (stamp DATETIME(6)) ENGINE=InnoDB")
            if entry == "callproc":
                setup.execute(f"CREATE PROCEDURE `{procedure}`(IN p DATETIME(6), IN q DATETIME(6)) "
                              f"INSERT INTO `{name}` VALUES (p)")
        context = connection.transaction(kind, prepared=prepared)
        current = context.__enter__()
        exited = False
        exits: list[InterfaceBusyError] = []
        later_calls: list[str] = []
        try:
            current.execute(f"INSERT INTO `{name}` VALUES (%s)", (datetime(2024, 1, 1),))

            def finish() -> None:
                nonlocal exited
                try:
                    context.__exit__(None, None, None)
                except InterfaceBusyError as error:
                    exits.append(error)
                finally:
                    exited = True
                assert current.closed
                if raises:
                    raise original

            class Zone(tzinfo):
                def utcoffset(self, dt: datetime | None) -> timedelta:
                    finish()
                    return timedelta(0)

            class Year:
                def __index__(self) -> int:
                    finish()
                    return 2024

            class LaterZone(tzinfo):
                def utcoffset(self, dt: datetime | None) -> timedelta:
                    later_calls.append("offset")
                    return timedelta(0)

            class Month:
                def __index__(self) -> int:
                    later_calls.append("month index")
                    return 1

            fields: Any = (Year(), Month(), 2, 0, 0, 0, 1, 2, 0)
            if source_kind == "datetime":
                value = datetime(2024, 1, 2, tzinfo=Zone())
            elif source_kind == "struct-time":
                value = time.struct_time(fields)
            elif source_kind == "json-dict":
                value = JSON({"first": datetime(2024, 1, 2, tzinfo=Zone()),
                              "later": datetime(2030, 1, 1, tzinfo=LaterZone())})
            else:
                value = JSON(clock_time(3, tzinfo=Zone()))
            caught: Exception | None = None
            try:
                if entry == "execute":
                    current.execute(f"INSERT INTO `{name}` VALUES (COALESCE(%s,%s))",
                                    (value, datetime(2030, 1, 1, tzinfo=LaterZone())))
                else:
                    current.callproc(procedure, (value, datetime(2030, 1, 1, tzinfo=LaterZone())))
            except Exception as error:
                caught = error
            assert exited and len(exits) == 1 and current.closed and not connection.closed
            # Check execution facts before the exception assertion, so the red
            # baseline records a real unwanted write rather than only no error.
            with connection.cursor() as check:
                check.execute(f"SELECT COUNT(*) FROM `{name}`")
                assert check.fetchone() == (1,)
            assert not later_calls, "a later source protocol ran after retirement"
            if raises:
                assert caught is original
            else:
                assert isinstance(caught, InterfaceError)
                assert caught.code == SQLAeroErrorCode.SESSION_STATE
                assert caught.is_server_error is False
                assert "closed" in str(caught) or "retired" in str(caught)
            # The original insertion still belongs to the uncommitted actual
            # transaction; the retired callback's second write never happened.
            connection.rollback()
            with connection.cursor() as check:
                check.execute(f"SELECT COUNT(*) FROM `{name}`")
                assert check.fetchone() == (0,)
        finally:
            if not exited:
                context.__exit__(None, None, None)
            if not connection.closed:
                connection.rollback()
                with connection.cursor() as cleanup:
                    if entry == "callproc":
                        cleanup.execute(f"DROP PROCEDURE `{procedure}`")
                    cleanup.execute(f"DROP TEMPORARY TABLE `{name}`")
