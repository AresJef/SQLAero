"""Replication signatures and execution against a standalone MySQL server."""

from collections.abc import Callable
from inspect import signature

import pytest
from sqlaero import Connection, sqlfunc
from sqlaero.errors import SQLAeroError
from sqlaero.typing import EscapeInput

from test_public_connection import connection as connection, requires_mysql


MEMBER = "f2a34740-cc6b-41d7-89d5-e28e360e8b84"
CHANNEL = "sqlaero"
ACTION = "mysql_disable_super_read_only_if_primary"
EVENT = "AFTER_PRIMARY_ELECTION"

CASES: tuple[tuple[str, tuple[object, ...]], ...] = (
    ("asynchronous_connection_failover_add_managed", (CHANNEL, "GroupReplication", MEMBER, "127.0.0.1", 1, "", 80, 60)),
    ("asynchronous_connection_failover_add_source", (CHANNEL, "127.0.0.1", 1, "", 60)),
    ("asynchronous_connection_failover_delete_managed", (CHANNEL, MEMBER)),
    ("asynchronous_connection_failover_delete_source", (CHANNEL, "127.0.0.1", 1, "")),
    ("asynchronous_connection_failover_reset", ()),
    ("group_replication_disable_member_action", (ACTION, EVENT)),
    ("group_replication_enable_member_action", (ACTION, EVENT)),
    ("group_replication_get_communication_protocol", ()),
    ("group_replication_get_write_concurrency", ()),
    ("group_replication_reset_member_actions", ()),
    ("group_replication_set_as_primary", (MEMBER, 1)),
    ("group_replication_set_communication_protocol", ("8.0.27",)),
    ("group_replication_set_write_concurrency", (10,)),
    ("group_replication_switch_to_multi_primary_mode", ()),
    ("group_replication_switch_to_single_primary_mode", (MEMBER,)),
    ("MASTER_POS_WAIT", ("binlog.000001", 4, 1, CHANNEL)),
    ("SOURCE_POS_WAIT", ("binlog.000001", 4, 1, CHANNEL)),
    ("WAIT_FOR_EXECUTED_GTID_SET", ("", 1)),
)


def test_replication_catalog_and_options() -> None:
    assert len({name for name, _ in CASES}) == 18
    for name, arguments in CASES:
        cls: type[object] = getattr(sqlfunc, name)
        assert name in sqlfunc.__all__ and cls.__module__ == "sqlaero.sqlfunc"
        assert cls.__base__ is sqlfunc.ABS.__base__ and cls.__doc__
        signature(cls).bind(*arguments)
    with pytest.raises(TypeError, match="omitted"):
        sqlfunc.SOURCE_POS_WAIT("binlog.000001", 4, channel=CHANNEL)
    assert signature(sqlfunc.MASTER_POS_WAIT) == signature(sqlfunc.SOURCE_POS_WAIT)


@requires_mysql
def test_replication_grammar_and_standalone_capabilities(connection: Connection) -> None:
    with connection.cursor() as cursor:
        for name, arguments in CASES:
            constructor: Callable[..., EscapeInput] = getattr(sqlfunc, name)
            expr = constructor(*arguments)
            sql = str(connection.escape(expr))
            assert sql.startswith(name + "(") and sql.endswith(")")
        for constructor in (sqlfunc.MASTER_POS_WAIT, sqlfunc.SOURCE_POS_WAIT):
            cursor.execute("SELECT %s", constructor("binlog.000001", 4, 1))
            assert cursor.fetchone() == (None,)
        cursor.execute("SELECT @@GLOBAL.gtid_mode")
        mode = cursor.fetchone()
        assert mode is not None
        wait = sqlfunc.WAIT_FOR_EXECUTED_GTID_SET("", 1)
        if mode[0] == "OFF":
            with pytest.raises(SQLAeroError, match="GTID_MODE = OFF") as caught:
                cursor.execute("SELECT %s", wait)
            assert caught.value.is_server_error
        else:
            cursor.execute("SELECT %s", wait)
            assert cursor.fetchone() == (0,)
        for constructor in (
            sqlfunc.group_replication_get_communication_protocol,
            sqlfunc.group_replication_get_write_concurrency,
        ):
            with pytest.raises(SQLAeroError) as caught:
                cursor.execute("SELECT %s", constructor())
            assert caught.value.is_server_error
