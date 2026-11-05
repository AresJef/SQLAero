"""Protocol option changes share the real command and confirmed-fact path."""

from typing import Any

import pytest
import sqlaero
from sqlaero.errors import InterfaceError, ProgrammingError
from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("compression", [None, "zlib", "zstd"])
def test_multi_statement_permission_changes_only_the_text_protocol(label: str, compression: Any) -> None:
    with live(label, compression=compression, multi_statements=False) as connection:
        assert connection.multi_statements is False
        with connection.cursor() as cursor:
            assert cursor.execute("SELECT 1") == 1
            assert cursor.fetchone() == (1,)
            with pytest.raises(ProgrammingError):
                cursor.execute("SELECT 1; SELECT 2")
            assert connection.multi_statements is False
            assert connection.set_multi_statements(True) is None
            assert connection.multi_statements is True
            assert cursor.execute("SELECT 1; SELECT 2") == 1
            assert cursor.fetchone() == (1,)
            assert cursor.nextset() is True
            assert cursor.fetchone() == (2,)
            assert cursor.nextset() is None
            connection.begin()
            with pytest.raises(InterfaceError):
                with connection.transaction():
                    pass
            assert connection.set_multi_statements(False) is None
            with pytest.raises(InterfaceError):
                with connection.transaction():
                    pass
            assert connection.multi_statements is False
            connection.rollback()
            assert connection.set_multi_statements(False) is None
        connection.close()
        assert connection.multi_statements is False
        with pytest.raises(sqlaero.errors.InterfaceError):
            connection.set_multi_statements(False)


@requires_mysql
@pytest.mark.parametrize("value", [None, 0, 1, "true", [], 0.5])
def test_multi_statement_setter_is_strict_and_read_only(value: object) -> None:
    with live("84") as connection:
        with pytest.raises(TypeError):
            connection.set_multi_statements(value)
        with pytest.raises(AttributeError):
            connection.multi_statements = False
        assert connection.multi_statements is True
