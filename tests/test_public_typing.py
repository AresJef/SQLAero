"""Validate public installed declarations against actual native boundaries."""

# Private import is used only to verify public identity-preserving re-exports.
# pyright: reportPrivateUsage=false

import inspect
import json
from pathlib import Path
import subprocess
import sys
from typing import Any

import sqlaero
from sqlaero import Connection, Cursor


def test_public_stub_modules_export_the_same_native_identities() -> None:
    from sqlaero import _sqlaero
    from sqlaero import errors
    from sqlaero.error_codes import MySQLErrorCode, SQLAeroErrorCode

    package = Path(sqlaero.__file__).parent
    assert (package / '__init__.pyi').is_file()
    assert (package / 'errors.pyi').is_file()
    assert errors.MySQLErrorCode is MySQLErrorCode is _sqlaero.MySQLErrorCode
    assert errors.SQLAeroErrorCode is SQLAeroErrorCode is _sqlaero.SQLAeroErrorCode
    assert sqlaero.Connection is _sqlaero.Connection


def test_installed_types_reject_forbidden_construction_and_subclassing(tmp_path: Path) -> None:
    """Use the real Pyright executable, not string checks on the stub source."""
    positive = '''from typing import Literal, assert_type
from sqlaero import connect, Connection, Cursor, Columns, ColumnInfo, SSL
from sqlaero.errors import SQLAeroError, ErrorInfo, OperationalError, MySQLErrorCode
from sqlaero.retry import Retry, RetryRule, retry
connection = connect(user="type check only", ssl=SSL())
assert_type(connection, Connection)
assert_type(connection.ssl_mode, Literal["DISABLED", "REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"])
assert_type(connection.tls_version, Literal["TLSv1.2", "TLSv1.3"] | None)
def accepts_mode(_mode: Literal["DISABLED", "REQUIRED", "VERIFY_CA", "VERIFY_IDENTITY"]) -> None:
    pass
accepts_mode(connection.ssl_mode)
cursor = connection.cursor()
assert_type(cursor, Cursor)
assert_type(cursor.columns, Columns)
assert_type(cursor.columns[0], ColumnInfo)
assert_type(SQLAeroError().error_info, ErrorInfo)
assert_type(SQLAeroError(code=MySQLErrorCode.ER_NON_UNIQ_ERROR).code, int | None)
for attempt in Retry(RetryRule(error_types=OperationalError)):
    with attempt:
        pass
@retry(RetryRule(error_types=OperationalError))
def decorated(value: int, *, enabled: bool = True) -> str:
    return str(value) if enabled else "disabled"
assert_type(decorated(1, enabled=True), str)
class ApplicationError(SQLAeroError):
    pass
'''
    imports = [
        'from sqlaero import Connection, Cursor, Columns, ColumnInfo, SSL',
        'from sqlaero.errors import ErrorInfo',
        'from sqlaero.retry import Retry, RetryRule, RetryAttempt',
    ]
    constructors = ['Connection', 'Cursor', 'Columns', 'ColumnInfo', 'ErrorInfo', 'RetryAttempt']
    final_classes = ['SSL', *constructors, 'Retry', 'RetryRule']
    negative = imports + [f'{name}()' for name in constructors]
    negative += [f'class Invalid{name}({name}): pass' for name in final_classes]
    (tmp_path / 'positive.py').write_text(positive)
    (tmp_path / 'negative.py').write_text('\n'.join(negative) + '\n')
    settings = {'include': ['positive.py', 'negative.py'], 'typeCheckingMode': 'strict',
                'pythonVersion': f'{sys.version_info.major}.{sys.version_info.minor}',
                'extraPaths': [str(Path(sqlaero.__file__).parent.parent)]}
    configuration = tmp_path / 'pyrightconfig.json'
    configuration.write_text(json.dumps(settings))
    result = subprocess.run(['pyright', '--outputjson', '--pythonpath', sys.executable,
                             '--project', str(configuration)], text=True, capture_output=True,
                            timeout=30, check=False)
    report: dict[str, Any] = json.loads(result.stdout)
    errors = [entry for entry in report['generalDiagnostics'] if entry['severity'] == 'error']
    assert result.returncode == 1 and len(errors) == 15, result.stdout + result.stderr
    assert all(Path(entry['file']).name == 'negative.py' for entry in errors), result.stdout
    assert {entry['range']['start']['line'] for entry in errors} == set(range(3, 18)), result.stdout


def test_decode_and_close_docs_explain_both_values_and_deadline_boundaries() -> None:
    options = ('decode_json', 'decode_set', 'decode_bit', 'use_decimal',
               'use_timedelta', 'decode_text_strict', 'decode_date_strict')
    for cls in (Connection, Cursor):
        for name in options:
            doc = inspect.getdoc(getattr(cls, name)) or ''
            assert all(word in doc for word in ('True', 'False', 'Default', 'bool', 'close')), (cls, name, doc)
    close = inspect.getdoc(Connection.close) or ''
    assert all(word in close for word in ('None', 'read_timeout', 'write_timeout',
                                          'operation_timeout', 'cleanup', 'budget')), close
