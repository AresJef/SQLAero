"""Managed exits drain their own unread response before transaction control."""

import pytest

from sqlaero.errors import OperationalTimeoutError, SQLAeroError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_successful_transaction_drains_before_commit_and_closes_only_owned_cursor(label: str, dictionary: bool) -> None:
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE ss_tx (v INT) ENGINE=InnoDB")
        with connection.transaction(dict if dictionary else tuple, streaming=True) as cursor:
            assert cursor.execute("INSERT ss_tx VALUES (1),(2)") == 2
            assert cursor.execute("SELECT v FROM ss_tx ORDER BY v") is None
            metadata = cursor.columns
        assert cursor.closed and cursor.columns is metadata
        assert cursor.row_count == cursor.batch_row_count == 2
        assert connection.closed is False
        with connection.cursor() as check:
            check.execute("SELECT v FROM ss_tx ORDER BY v")
            assert check.fetchall() == ((1,), (2,))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_business_error_survives_drain_then_actual_rollback(label: str, dictionary: bool) -> None:
    primary = RuntimeError("original business failure")
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE ss_tx (v INT) ENGINE=InnoDB")
        with pytest.raises(RuntimeError) as caught:
            with connection.transaction(dict if dictionary else tuple, streaming=True) as cursor:
                cursor.execute("INSERT ss_tx VALUES (1),(2)")
                cursor.execute("SELECT v FROM ss_tx ORDER BY v")
                assert cursor.fetchone() == ({"v": 1} if dictionary else (1,))
                raise primary
        assert caught.value is primary and cursor.closed
        assert any("ROLLBACK" in note and "confirmed" in note for note in primary.__notes__)
        with connection.cursor() as check:
            check.execute("SELECT COUNT(*) FROM ss_tx")
            assert check.fetchone() == (0,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("dictionary", [False, True])
def test_server_error_found_during_exit_rolls_back_and_remains_the_primary(label: str, dictionary: bool) -> None:
    with live(label) as connection:
        with connection.cursor() as setup:
            setup.execute("CREATE TEMPORARY TABLE ss_tx (v INT) ENGINE=InnoDB")
        with pytest.raises(SQLAeroError) as caught:
            with connection.transaction(dict if dictionary else tuple, streaming=True) as cursor:
                assert cursor.execute("INSERT ss_tx VALUES (1),(2); SELECT v FROM ss_tx; SIGNAL SQLSTATE '45000' SET MYSQL_ERRNO=1644,MESSAGE_TEXT='late stream failure'") == 2
        assert caught.value.code == 1644 and caught.value.is_server_error is True
        assert "late stream failure" in str(caught.value)
        assert any("ROLLBACK" in note and "confirmed" in note for note in caught.value.__notes__)
        assert cursor.closed and connection.closed is False
        with connection.cursor() as check:
            check.execute("SELECT COUNT(*) FROM ss_tx")
            assert check.fetchone() == (0,)


@requires_mysql
def test_expired_exit_cap_does_not_grant_old_stream_a_fresh_cleanup_window() -> None:
    connection = live("84", operation_timeout=1)
    with pytest.raises(OperationalTimeoutError) as caught:
        with connection.transaction(streaming=True) as cursor:
            cursor.execute("SELECT 1 UNION ALL SELECT 2")
            connection.operation_timeout = 1e-9
    assert cursor.closed and connection.closed
    assert caught.value.is_server_error is False
    assert any("COMMIT was not attempted" in note for note in caught.value.__notes__)
    connection.close()
