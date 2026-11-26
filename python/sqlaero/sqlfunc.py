"""MySQL function expressions for text queries."""

from ._sqlaero import _sqlfunc  # pyright: ignore[reportPrivateUsage]

ABS = _sqlfunc.ABS
ACOS = _sqlfunc.ACOS
ASIN = _sqlfunc.ASIN
ATAN = _sqlfunc.ATAN
ATAN2 = _sqlfunc.ATAN2
BIT_COUNT = _sqlfunc.BIT_COUNT
CEIL = _sqlfunc.CEIL
CONV = _sqlfunc.CONV
COS = _sqlfunc.COS
COT = _sqlfunc.COT
CRC32 = _sqlfunc.CRC32
DATE = _sqlfunc.DATE
DATE_ADD = _sqlfunc.DATE_ADD
DEGREES = _sqlfunc.DEGREES
EXP = _sqlfunc.EXP
EXTRACT = _sqlfunc.EXTRACT
FLOOR = _sqlfunc.FLOOR
LN = _sqlfunc.LN
LOG = _sqlfunc.LOG
LOG10 = _sqlfunc.LOG10
LOG2 = _sqlfunc.LOG2
MOD = _sqlfunc.MOD
PI = _sqlfunc.PI
POW = _sqlfunc.POW
RADIANS = _sqlfunc.RADIANS
RAND = _sqlfunc.RAND
ROUND = _sqlfunc.ROUND
SIGN = _sqlfunc.SIGN
SIN = _sqlfunc.SIN
SQRT = _sqlfunc.SQRT
SUM = _sqlfunc.SUM
TAN = _sqlfunc.TAN
TRUNCATE = _sqlfunc.TRUNCATE

__all__: list[str] = [
    'ABS', 'ACOS', 'ASIN', 'ATAN', 'ATAN2', 'BIT_COUNT', 'CEIL',
    'CONV', 'COS', 'COT', 'CRC32', 'DATE', 'DATE_ADD', 'DEGREES',
    'EXP', 'EXTRACT', 'FLOOR', 'LN', 'LOG', 'LOG10', 'LOG2',
    'MOD', 'PI', 'POW', 'RADIANS', 'RAND', 'ROUND', 'SIGN',
    'SIN', 'SQRT', 'SUM', 'TAN', 'TRUNCATE',
]
