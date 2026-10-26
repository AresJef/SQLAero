"""SQLAero's declared exception identities shared with the native boundary."""

from ._sqlaero import (
    SQLAeroError, InterfaceError, InterfaceBusyError, DatabaseError, DataError,
    OperationalError, OperationalTimeoutError, IntegrityError, InternalError,
    ProgrammingError, NotSupportedError, SQLAeroWarning, ErrorInfo,
)

from .error_codes import SQLAeroErrorCode, MySQLErrorCode

__all__: list[str] = [
    'SQLAeroError', 'InterfaceError', 'InterfaceBusyError', 'DatabaseError', 'DataError',
    'OperationalError', 'OperationalTimeoutError', 'IntegrityError', 'InternalError',
    'ProgrammingError', 'NotSupportedError', 'SQLAeroWarning', 'ErrorInfo', 'SQLAeroErrorCode', 'MySQLErrorCode',
]
