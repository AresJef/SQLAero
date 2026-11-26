"""MySQL function expressions for text queries."""

from ._sqlaero import _sqlfunc  # pyright: ignore[reportPrivateUsage]

ABS = _sqlfunc.ABS
ACOS = _sqlfunc.ACOS
ASCII = _sqlfunc.ASCII
ASIN = _sqlfunc.ASIN
ATAN = _sqlfunc.ATAN
ATAN2 = _sqlfunc.ATAN2
BIN = _sqlfunc.BIN
BIT_COUNT = _sqlfunc.BIT_COUNT
BIT_LENGTH = _sqlfunc.BIT_LENGTH
CEIL = _sqlfunc.CEIL
CHAR = _sqlfunc.CHAR
CHAR_LENGTH = _sqlfunc.CHAR_LENGTH
CONCAT = _sqlfunc.CONCAT
CONCAT_WS = _sqlfunc.CONCAT_WS
CONV = _sqlfunc.CONV
COS = _sqlfunc.COS
COT = _sqlfunc.COT
CRC32 = _sqlfunc.CRC32
DATE = _sqlfunc.DATE
DATE_ADD = _sqlfunc.DATE_ADD
DEGREES = _sqlfunc.DEGREES
ELT = _sqlfunc.ELT
EXP = _sqlfunc.EXP
EXPORT_SET = _sqlfunc.EXPORT_SET
EXTRACT = _sqlfunc.EXTRACT
FIELD = _sqlfunc.FIELD
FIND_IN_SET = _sqlfunc.FIND_IN_SET
FLOOR = _sqlfunc.FLOOR
FORMAT = _sqlfunc.FORMAT
FROM_BASE64 = _sqlfunc.FROM_BASE64
HEX = _sqlfunc.HEX
INSERT = _sqlfunc.INSERT
INSTR = _sqlfunc.INSTR
LEFT = _sqlfunc.LEFT
LENGTH = _sqlfunc.LENGTH
LN = _sqlfunc.LN
LOCATE = _sqlfunc.LOCATE
LOG = _sqlfunc.LOG
LOG10 = _sqlfunc.LOG10
LOG2 = _sqlfunc.LOG2
LOWER = _sqlfunc.LOWER
LPAD = _sqlfunc.LPAD
LTRIM = _sqlfunc.LTRIM
MAKE_SET = _sqlfunc.MAKE_SET
MOD = _sqlfunc.MOD
OCT = _sqlfunc.OCT
ORD = _sqlfunc.ORD
PI = _sqlfunc.PI
POSITION = _sqlfunc.POSITION
POW = _sqlfunc.POW
QUOTE = _sqlfunc.QUOTE
RADIANS = _sqlfunc.RADIANS
RAND = _sqlfunc.RAND
REGEXP_INSTR = _sqlfunc.REGEXP_INSTR
REGEXP_LIKE = _sqlfunc.REGEXP_LIKE
REGEXP_REPLACE = _sqlfunc.REGEXP_REPLACE
REGEXP_SUBSTR = _sqlfunc.REGEXP_SUBSTR
REPEAT = _sqlfunc.REPEAT
REPLACE = _sqlfunc.REPLACE
REVERSE = _sqlfunc.REVERSE
RIGHT = _sqlfunc.RIGHT
ROUND = _sqlfunc.ROUND
RPAD = _sqlfunc.RPAD
RTRIM = _sqlfunc.RTRIM
SIGN = _sqlfunc.SIGN
SIN = _sqlfunc.SIN
SOUNDEX = _sqlfunc.SOUNDEX
SPACE = _sqlfunc.SPACE
SQRT = _sqlfunc.SQRT
STRCMP = _sqlfunc.STRCMP
SUBSTRING = _sqlfunc.SUBSTRING
SUBSTRING_INDEX = _sqlfunc.SUBSTRING_INDEX
SUM = _sqlfunc.SUM
TAN = _sqlfunc.TAN
TO_BASE64 = _sqlfunc.TO_BASE64
TRIM = _sqlfunc.TRIM
TRUNCATE = _sqlfunc.TRUNCATE
UNHEX = _sqlfunc.UNHEX
UPPER = _sqlfunc.UPPER

__all__: list[str] = [
    'ABS', 'ACOS', 'ASCII', 'ASIN', 'ATAN', 'ATAN2', 'BIN',
    'BIT_COUNT', 'BIT_LENGTH', 'CEIL', 'CHAR', 'CHAR_LENGTH', 'CONCAT', 'CONCAT_WS',
    'CONV', 'COS', 'COT', 'CRC32', 'DATE', 'DATE_ADD', 'DEGREES',
    'ELT', 'EXP', 'EXPORT_SET', 'EXTRACT', 'FIELD', 'FIND_IN_SET', 'FLOOR',
    'FORMAT', 'FROM_BASE64', 'HEX', 'INSERT', 'INSTR', 'LEFT', 'LENGTH',
    'LN', 'LOCATE', 'LOG', 'LOG10', 'LOG2', 'LOWER', 'LPAD',
    'LTRIM', 'MAKE_SET', 'MOD', 'OCT', 'ORD', 'PI', 'POSITION',
    'POW', 'QUOTE', 'RADIANS', 'RAND', 'REGEXP_INSTR', 'REGEXP_LIKE', 'REGEXP_REPLACE',
    'REGEXP_SUBSTR', 'REPEAT', 'REPLACE', 'REVERSE', 'RIGHT', 'ROUND', 'RPAD',
    'RTRIM', 'SIGN', 'SIN', 'SOUNDEX', 'SPACE', 'SQRT', 'STRCMP',
    'SUBSTRING', 'SUBSTRING_INDEX', 'SUM', 'TAN', 'TO_BASE64', 'TRIM', 'TRUNCATE',
    'UNHEX', 'UPPER',
]
