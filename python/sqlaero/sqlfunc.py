"""MySQL function expressions for text queries."""

from ._sqlaero import _sqlfunc  # pyright: ignore[reportPrivateUsage]

ABS = _sqlfunc.ABS
ACOS = _sqlfunc.ACOS
AES_DECRYPT = _sqlfunc.AES_DECRYPT
AES_ENCRYPT = _sqlfunc.AES_ENCRYPT
ASCII = _sqlfunc.ASCII
ASIN = _sqlfunc.ASIN
ATAN = _sqlfunc.ATAN
ATAN2 = _sqlfunc.ATAN2
BIN = _sqlfunc.BIN
BIN_TO_UUID = _sqlfunc.BIN_TO_UUID
BIT_COUNT = _sqlfunc.BIT_COUNT
BIT_LENGTH = _sqlfunc.BIT_LENGTH
CAST = _sqlfunc.CAST
CEIL = _sqlfunc.CEIL
CHAR = _sqlfunc.CHAR
CHARSET = _sqlfunc.CHARSET
CHAR_LENGTH = _sqlfunc.CHAR_LENGTH
COALESCE = _sqlfunc.COALESCE
COERCIBILITY = _sqlfunc.COERCIBILITY
COLLATION = _sqlfunc.COLLATION
COMPRESS = _sqlfunc.COMPRESS
CONCAT = _sqlfunc.CONCAT
CONCAT_WS = _sqlfunc.CONCAT_WS
CONNECTION_ID = _sqlfunc.CONNECTION_ID
CONV = _sqlfunc.CONV
CONVERT = _sqlfunc.CONVERT
COS = _sqlfunc.COS
COT = _sqlfunc.COT
CRC32 = _sqlfunc.CRC32
CURRENT_ROLE = _sqlfunc.CURRENT_ROLE
CURRENT_USER = _sqlfunc.CURRENT_USER
DATABASE = _sqlfunc.DATABASE
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
GREATEST = _sqlfunc.GREATEST
HEX = _sqlfunc.HEX
ICU_VERSION = _sqlfunc.ICU_VERSION
IF = _sqlfunc.IF
IFNULL = _sqlfunc.IFNULL
INET6_ATON = _sqlfunc.INET6_ATON
INET6_NTOA = _sqlfunc.INET6_NTOA
INET_ATON = _sqlfunc.INET_ATON
INET_NTOA = _sqlfunc.INET_NTOA
INSERT = _sqlfunc.INSERT
INSTR = _sqlfunc.INSTR
INTERVAL = _sqlfunc.INTERVAL
ISNULL = _sqlfunc.ISNULL
IS_IPV4 = _sqlfunc.IS_IPV4
IS_IPV4_COMPAT = _sqlfunc.IS_IPV4_COMPAT
IS_IPV4_MAPPED = _sqlfunc.IS_IPV4_MAPPED
IS_IPV6 = _sqlfunc.IS_IPV6
IS_UUID = _sqlfunc.IS_UUID
LAST_INSERT_ID = _sqlfunc.LAST_INSERT_ID
LEAST = _sqlfunc.LEAST
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
NULLIF = _sqlfunc.NULLIF
OCT = _sqlfunc.OCT
ORD = _sqlfunc.ORD
PI = _sqlfunc.PI
POSITION = _sqlfunc.POSITION
POW = _sqlfunc.POW
QUOTE = _sqlfunc.QUOTE
RADIANS = _sqlfunc.RADIANS
RAND = _sqlfunc.RAND
RANDOM_BYTES = _sqlfunc.RANDOM_BYTES
REGEXP_INSTR = _sqlfunc.REGEXP_INSTR
REGEXP_LIKE = _sqlfunc.REGEXP_LIKE
REGEXP_REPLACE = _sqlfunc.REGEXP_REPLACE
REGEXP_SUBSTR = _sqlfunc.REGEXP_SUBSTR
REPEAT = _sqlfunc.REPEAT
REPLACE = _sqlfunc.REPLACE
REVERSE = _sqlfunc.REVERSE
RIGHT = _sqlfunc.RIGHT
ROUND = _sqlfunc.ROUND
ROW_COUNT = _sqlfunc.ROW_COUNT
RPAD = _sqlfunc.RPAD
RTRIM = _sqlfunc.RTRIM
SHA2 = _sqlfunc.SHA2
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
UNCOMPRESS = _sqlfunc.UNCOMPRESS
UNCOMPRESSED_LENGTH = _sqlfunc.UNCOMPRESSED_LENGTH
UNHEX = _sqlfunc.UNHEX
UPPER = _sqlfunc.UPPER
USER = _sqlfunc.USER
UUID = _sqlfunc.UUID
UUID_SHORT = _sqlfunc.UUID_SHORT
UUID_TO_BIN = _sqlfunc.UUID_TO_BIN
VERSION = _sqlfunc.VERSION

__all__: list[str] = [
    'ABS', 'ACOS', 'AES_DECRYPT', 'AES_ENCRYPT', 'ASCII', 'ASIN', 'ATAN',
    'ATAN2', 'BIN', 'BIN_TO_UUID', 'BIT_COUNT', 'BIT_LENGTH', 'CAST', 'CEIL',
    'CHAR', 'CHARSET', 'CHAR_LENGTH', 'COALESCE', 'COERCIBILITY', 'COLLATION', 'COMPRESS',
    'CONCAT', 'CONCAT_WS', 'CONNECTION_ID', 'CONV', 'CONVERT', 'COS', 'COT',
    'CRC32', 'CURRENT_ROLE', 'CURRENT_USER', 'DATABASE', 'DATE', 'DATE_ADD', 'DEGREES',
    'ELT', 'EXP', 'EXPORT_SET', 'EXTRACT', 'FIELD', 'FIND_IN_SET', 'FLOOR',
    'FORMAT', 'FROM_BASE64', 'GREATEST', 'HEX', 'ICU_VERSION', 'IF', 'IFNULL',
    'INET6_ATON', 'INET6_NTOA', 'INET_ATON', 'INET_NTOA', 'INSERT', 'INSTR', 'INTERVAL',
    'ISNULL', 'IS_IPV4', 'IS_IPV4_COMPAT', 'IS_IPV4_MAPPED', 'IS_IPV6', 'IS_UUID', 'LAST_INSERT_ID',
    'LEAST', 'LEFT', 'LENGTH', 'LN', 'LOCATE', 'LOG', 'LOG10',
    'LOG2', 'LOWER', 'LPAD', 'LTRIM', 'MAKE_SET', 'MOD', 'NULLIF',
    'OCT', 'ORD', 'PI', 'POSITION', 'POW', 'QUOTE', 'RADIANS',
    'RAND', 'RANDOM_BYTES', 'REGEXP_INSTR', 'REGEXP_LIKE', 'REGEXP_REPLACE', 'REGEXP_SUBSTR', 'REPEAT',
    'REPLACE', 'REVERSE', 'RIGHT', 'ROUND', 'ROW_COUNT', 'RPAD', 'RTRIM',
    'SHA2', 'SIGN', 'SIN', 'SOUNDEX', 'SPACE', 'SQRT', 'STRCMP',
    'SUBSTRING', 'SUBSTRING_INDEX', 'SUM', 'TAN', 'TO_BASE64', 'TRIM', 'TRUNCATE',
    'UNCOMPRESS', 'UNCOMPRESSED_LENGTH', 'UNHEX', 'UPPER', 'USER', 'UUID', 'UUID_SHORT',
    'UUID_TO_BIN', 'VERSION',
]
