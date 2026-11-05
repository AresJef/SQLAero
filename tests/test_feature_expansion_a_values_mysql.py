"""Database semantics of complete SQL requests produced by the real Planner.

These tests materialize final probe bytes before sending them through the
public connection. Public execute's incremental handoff is checked separately.
"""

import pytest
from sqlaero.errors import DataError
from test_feature_expansion_a_planner import plan
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_planned_multigroup_default_alias_odku_replace_and_row_effects(label: str) -> None:
    with live(label) as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE a_values_effects (id INT PRIMARY KEY, v INT DEFAULT 9, g INT AS (id+v))")
        sql = ("INSERT a_values_effects (id,v) VALUES (%s,DEFAULT),(%s,7) "
               "AS new(x,y) ON DUPLICATE KEY UPDATE v=new.y+1")
        requests = plan(sql, [(1, 2), (3, 4)], many=True)
        assert len(requests) == 1 and requests[0][1:] == (0, 2, True)
        assert cursor.execute(requests[0][0][1:].decode()) == 4
        cursor.execute("SELECT id,v,g FROM a_values_effects ORDER BY id")
        assert cursor.fetchall() == ((1, 9, 10), (2, 7, 9), (3, 9, 12), (4, 7, 11))
        assert cursor.execute(requests[0][0][1:].decode()) == 8
        cursor.execute("SELECT id,v,g FROM a_values_effects ORDER BY id")
        assert cursor.fetchall() == ((1, 10, 11), (2, 8, 10), (3, 10, 13), (4, 8, 12))
        requests = plan("REPLACE a_values_effects (id,v) VALUES ROW(%s,DEFAULT),ROW(%s,5)",
                        [(1, 2), (3, 4)], many=True)
        assert len(requests) == 1
        assert cursor.execute(requests[0][0][1:].decode()) == 8
        cursor.execute("SELECT id,v,g FROM a_values_effects ORDER BY id")
        assert cursor.fetchall() == ((1, 9, 10), (2, 5, 7), (3, 9, 12), (4, 5, 9))
        requests = plan("INSERT a_values_effects (id,v) VALUES %s AS new ON DUPLICATE KEY UPDATE v=new.v+1",
                        [(1, 20), (2, 30)], many=True, itemize=False)
        assert len(requests) == 1
        assert cursor.execute(requests[0][0][1:].decode()) == 4
        cursor.execute("SELECT id,v FROM a_values_effects WHERE id<=2 ORDER BY id")
        assert cursor.fetchall() == ((1, 21), (2, 31))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_fixed_partition_prefix_preserves_server_partition_constraints(label: str) -> None:
    with live(label) as connection, connection.cursor() as cursor:
        cursor.execute("DROP TABLE IF EXISTS a_values_partition")
        try:
            cursor.execute("CREATE TABLE a_values_partition (id INT PRIMARY KEY, v INT) "
                           "PARTITION BY RANGE(id) (PARTITION p0 VALUES LESS THAN (10), PARTITION p1 VALUES LESS THAN MAXVALUE)")
            requests = plan("INSERT a_values_partition PARTITION (p0,p1) VALUES (%s,1)", [1, 11], many=True)
            assert len(requests) == 1 and cursor.execute(requests[0][0][1:].decode()) == 2
            cursor.execute("SELECT id,v FROM a_values_partition ORDER BY id")
            assert cursor.fetchall() == ((1, 1), (11, 1))
            requests = plan("INSERT a_values_partition PARTITION (p0) VALUES (%s,2)", [2, 12], many=True)
            assert len(requests) == 1
            with pytest.raises(DataError) as failure:
                cursor.execute(requests[0][0][1:].decode())
            assert failure.value.code == 1748 and failure.value.is_server_error is True
            cursor.execute("SELECT id,v FROM a_values_partition ORDER BY id")
            assert cursor.fetchall() == ((1, 1), (11, 1))
        finally:
            cursor.execute("DROP TABLE IF EXISTS a_values_partition")
