"""User initialization must not hide changed facts or disable required tracking."""

import pytest

from sqlaero.errors import SQLAeroError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_checks_actual_utc_after_a_temporary_tracking_gap(label: str) -> None:
    result = None
    try:
        with pytest.raises(SQLAeroError, match="UTC|time_zone"):
            result = live(label, init_sql=(
                "SET SESSION session_track_system_variables=''",
                "SET SESSION time_zone='+08:00'",
                "SET SESSION session_track_system_variables='*'",
            ))
    finally:
        if result is not None:
            result.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_cannot_leave_required_tracking_disabled(label: str) -> None:
    result = None
    try:
        with pytest.raises(SQLAeroError, match="session.track|tracking"):
            result = live(label, init_sql="SET SESSION session_track_system_variables=''")
    finally:
        if result is not None:
            result.close()


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
def test_init_sql_final_confirmation_preserves_server_timeout_changes(label: str) -> None:
    with live(label, init_sql=(
        "SET SESSION session_track_system_variables=''",
        "SET SESSION wait_timeout=321",
        "SET SESSION session_track_system_variables='*'",
    )) as result:
        assert result.wait_timeout == 321
