"""Standard IntEnums sourced from SQLAero's single maintained native registry."""

from ._sqlaero import MySQLErrorCode, SQLAeroErrorCode

__all__: list[str] = ['MySQLErrorCode', 'SQLAeroErrorCode']
