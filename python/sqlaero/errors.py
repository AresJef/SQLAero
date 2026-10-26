"""SQLAero's declared exception identities shared with the native boundary."""

from ._sqlaero import (
    SQLAeroError, InterfaceError, InterfaceBusyError, DatabaseError, DataError,
    OperationalError, OperationalTimeoutError, IntegrityError, InternalError,
    ProgrammingError, NotSupportedError, SQLAeroWarning,
)

__all__: list[str] = [
    'SQLAeroError', 'InterfaceError', 'InterfaceBusyError', 'DatabaseError', 'DataError',
    'OperationalError', 'OperationalTimeoutError', 'IntegrityError', 'InternalError',
    'ProgrammingError', 'NotSupportedError', 'SQLAeroWarning',
]
