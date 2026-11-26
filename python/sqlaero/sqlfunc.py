"""MySQL function expressions for text queries."""

from ._sqlaero import _sqlfunc  # pyright: ignore[reportPrivateUsage]

ABS = _sqlfunc.ABS
ACOS = _sqlfunc.ACOS
ADDDATE = _sqlfunc.ADDDATE
ADDTIME = _sqlfunc.ADDTIME
AES_DECRYPT = _sqlfunc.AES_DECRYPT
AES_ENCRYPT = _sqlfunc.AES_ENCRYPT
ANY_VALUE = _sqlfunc.ANY_VALUE
ASCII = _sqlfunc.ASCII
ASIN = _sqlfunc.ASIN
ATAN = _sqlfunc.ATAN
ATAN2 = _sqlfunc.ATAN2
AVG = _sqlfunc.AVG
BIN = _sqlfunc.BIN
BIN_TO_UUID = _sqlfunc.BIN_TO_UUID
BIT_AND = _sqlfunc.BIT_AND
BIT_COUNT = _sqlfunc.BIT_COUNT
BIT_LENGTH = _sqlfunc.BIT_LENGTH
BIT_OR = _sqlfunc.BIT_OR
BIT_XOR = _sqlfunc.BIT_XOR
CAST = _sqlfunc.CAST
CEIL = _sqlfunc.CEIL
CEILING = _sqlfunc.CEILING
CHAR = _sqlfunc.CHAR
CHARACTER_LENGTH = _sqlfunc.CHARACTER_LENGTH
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
CONVERT_TZ = _sqlfunc.CONVERT_TZ
COS = _sqlfunc.COS
COT = _sqlfunc.COT
COUNT = _sqlfunc.COUNT
CRC32 = _sqlfunc.CRC32
CUME_DIST = _sqlfunc.CUME_DIST
CURDATE = _sqlfunc.CURDATE
CURRENT_DATE = _sqlfunc.CURRENT_DATE
CURRENT_ROLE = _sqlfunc.CURRENT_ROLE
CURRENT_TIME = _sqlfunc.CURRENT_TIME
CURRENT_TIMESTAMP = _sqlfunc.CURRENT_TIMESTAMP
CURRENT_USER = _sqlfunc.CURRENT_USER
CURTIME = _sqlfunc.CURTIME
DATABASE = _sqlfunc.DATABASE
DATE = _sqlfunc.DATE
DATEDIFF = _sqlfunc.DATEDIFF
DATE_ADD = _sqlfunc.DATE_ADD
DATE_FORMAT = _sqlfunc.DATE_FORMAT
DATE_SUB = _sqlfunc.DATE_SUB
DAY = _sqlfunc.DAY
DAYNAME = _sqlfunc.DAYNAME
DAYOFMONTH = _sqlfunc.DAYOFMONTH
DAYOFWEEK = _sqlfunc.DAYOFWEEK
DAYOFYEAR = _sqlfunc.DAYOFYEAR
DEGREES = _sqlfunc.DEGREES
DENSE_RANK = _sqlfunc.DENSE_RANK
ELT = _sqlfunc.ELT
EXP = _sqlfunc.EXP
EXPORT_SET = _sqlfunc.EXPORT_SET
EXTRACT = _sqlfunc.EXTRACT
FIELD = _sqlfunc.FIELD
FIND_IN_SET = _sqlfunc.FIND_IN_SET
FIRST_VALUE = _sqlfunc.FIRST_VALUE
FLOOR = _sqlfunc.FLOOR
FORMAT = _sqlfunc.FORMAT
FROM_BASE64 = _sqlfunc.FROM_BASE64
FROM_DAYS = _sqlfunc.FROM_DAYS
FROM_UNIXTIME = _sqlfunc.FROM_UNIXTIME
FROM_VECTOR = _sqlfunc.FROM_VECTOR
GET_FORMAT = _sqlfunc.GET_FORMAT
GREATEST = _sqlfunc.GREATEST
GROUPING = _sqlfunc.GROUPING
GROUP_CONCAT = _sqlfunc.GROUP_CONCAT
GeomCollection = _sqlfunc.GeomCollection
GeometryCollection = _sqlfunc.GeometryCollection
HEX = _sqlfunc.HEX
HOUR = _sqlfunc.HOUR
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
JSON_ARRAY = _sqlfunc.JSON_ARRAY
JSON_ARRAYAGG = _sqlfunc.JSON_ARRAYAGG
JSON_ARRAY_APPEND = _sqlfunc.JSON_ARRAY_APPEND
JSON_ARRAY_INSERT = _sqlfunc.JSON_ARRAY_INSERT
JSON_CONTAINS = _sqlfunc.JSON_CONTAINS
JSON_CONTAINS_PATH = _sqlfunc.JSON_CONTAINS_PATH
JSON_DEPTH = _sqlfunc.JSON_DEPTH
JSON_EXTRACT = _sqlfunc.JSON_EXTRACT
JSON_INSERT = _sqlfunc.JSON_INSERT
JSON_KEYS = _sqlfunc.JSON_KEYS
JSON_LENGTH = _sqlfunc.JSON_LENGTH
JSON_MERGE_PATCH = _sqlfunc.JSON_MERGE_PATCH
JSON_MERGE_PRESERVE = _sqlfunc.JSON_MERGE_PRESERVE
JSON_OBJECT = _sqlfunc.JSON_OBJECT
JSON_OBJECTAGG = _sqlfunc.JSON_OBJECTAGG
JSON_OVERLAPS = _sqlfunc.JSON_OVERLAPS
JSON_PRETTY = _sqlfunc.JSON_PRETTY
JSON_QUOTE = _sqlfunc.JSON_QUOTE
JSON_REMOVE = _sqlfunc.JSON_REMOVE
JSON_REPLACE = _sqlfunc.JSON_REPLACE
JSON_SCHEMA_VALID = _sqlfunc.JSON_SCHEMA_VALID
JSON_SCHEMA_VALIDATION_REPORT = _sqlfunc.JSON_SCHEMA_VALIDATION_REPORT
JSON_SEARCH = _sqlfunc.JSON_SEARCH
JSON_SET = _sqlfunc.JSON_SET
JSON_STORAGE_FREE = _sqlfunc.JSON_STORAGE_FREE
JSON_STORAGE_SIZE = _sqlfunc.JSON_STORAGE_SIZE
JSON_TYPE = _sqlfunc.JSON_TYPE
JSON_UNQUOTE = _sqlfunc.JSON_UNQUOTE
JSON_VALID = _sqlfunc.JSON_VALID
JSON_VALUE = _sqlfunc.JSON_VALUE
LAG = _sqlfunc.LAG
LAST_DAY = _sqlfunc.LAST_DAY
LAST_INSERT_ID = _sqlfunc.LAST_INSERT_ID
LAST_VALUE = _sqlfunc.LAST_VALUE
LCASE = _sqlfunc.LCASE
LEAD = _sqlfunc.LEAD
LEAST = _sqlfunc.LEAST
LEFT = _sqlfunc.LEFT
LENGTH = _sqlfunc.LENGTH
LN = _sqlfunc.LN
LOCALTIME = _sqlfunc.LOCALTIME
LOCALTIMESTAMP = _sqlfunc.LOCALTIMESTAMP
LOCATE = _sqlfunc.LOCATE
LOG = _sqlfunc.LOG
LOG10 = _sqlfunc.LOG10
LOG2 = _sqlfunc.LOG2
LOWER = _sqlfunc.LOWER
LPAD = _sqlfunc.LPAD
LTRIM = _sqlfunc.LTRIM
LineString = _sqlfunc.LineString
MAKEDATE = _sqlfunc.MAKEDATE
MAKETIME = _sqlfunc.MAKETIME
MAKE_SET = _sqlfunc.MAKE_SET
MAX = _sqlfunc.MAX
MBRContains = _sqlfunc.MBRContains
MBRCoveredBy = _sqlfunc.MBRCoveredBy
MBRCovers = _sqlfunc.MBRCovers
MBRDisjoint = _sqlfunc.MBRDisjoint
MBREquals = _sqlfunc.MBREquals
MBRIntersects = _sqlfunc.MBRIntersects
MBROverlaps = _sqlfunc.MBROverlaps
MBRTouches = _sqlfunc.MBRTouches
MBRWithin = _sqlfunc.MBRWithin
MICROSECOND = _sqlfunc.MICROSECOND
MID = _sqlfunc.MID
MIN = _sqlfunc.MIN
MINUTE = _sqlfunc.MINUTE
MOD = _sqlfunc.MOD
MONTH = _sqlfunc.MONTH
MONTHNAME = _sqlfunc.MONTHNAME
MultiLineString = _sqlfunc.MultiLineString
MultiPoint = _sqlfunc.MultiPoint
MultiPolygon = _sqlfunc.MultiPolygon
NOW = _sqlfunc.NOW
NTH_VALUE = _sqlfunc.NTH_VALUE
NTILE = _sqlfunc.NTILE
NULLIF = _sqlfunc.NULLIF
OCT = _sqlfunc.OCT
OCTET_LENGTH = _sqlfunc.OCTET_LENGTH
ORD = _sqlfunc.ORD
PERCENT_RANK = _sqlfunc.PERCENT_RANK
PERIOD_ADD = _sqlfunc.PERIOD_ADD
PERIOD_DIFF = _sqlfunc.PERIOD_DIFF
PI = _sqlfunc.PI
POSITION = _sqlfunc.POSITION
POW = _sqlfunc.POW
POWER = _sqlfunc.POWER
Point = _sqlfunc.Point
Polygon = _sqlfunc.Polygon
QUARTER = _sqlfunc.QUARTER
QUOTE = _sqlfunc.QUOTE
RADIANS = _sqlfunc.RADIANS
RAND = _sqlfunc.RAND
RANDOM_BYTES = _sqlfunc.RANDOM_BYTES
RANK = _sqlfunc.RANK
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
ROW_NUMBER = _sqlfunc.ROW_NUMBER
RPAD = _sqlfunc.RPAD
RTRIM = _sqlfunc.RTRIM
SCHEMA = _sqlfunc.SCHEMA
SECOND = _sqlfunc.SECOND
SEC_TO_TIME = _sqlfunc.SEC_TO_TIME
SESSION_USER = _sqlfunc.SESSION_USER
SHA2 = _sqlfunc.SHA2
SIGN = _sqlfunc.SIGN
SIN = _sqlfunc.SIN
SOUNDEX = _sqlfunc.SOUNDEX
SPACE = _sqlfunc.SPACE
SQRT = _sqlfunc.SQRT
STD = _sqlfunc.STD
STDDEV = _sqlfunc.STDDEV
STDDEV_POP = _sqlfunc.STDDEV_POP
STDDEV_SAMP = _sqlfunc.STDDEV_SAMP
STRCMP = _sqlfunc.STRCMP
STRING_TO_VECTOR = _sqlfunc.STRING_TO_VECTOR
STR_TO_DATE = _sqlfunc.STR_TO_DATE
ST_Area = _sqlfunc.ST_Area
ST_AsBinary = _sqlfunc.ST_AsBinary
ST_AsGeoJSON = _sqlfunc.ST_AsGeoJSON
ST_AsText = _sqlfunc.ST_AsText
ST_AsWKB = _sqlfunc.ST_AsWKB
ST_AsWKT = _sqlfunc.ST_AsWKT
ST_Buffer = _sqlfunc.ST_Buffer
ST_Buffer_Strategy = _sqlfunc.ST_Buffer_Strategy
ST_Centroid = _sqlfunc.ST_Centroid
ST_Collect = _sqlfunc.ST_Collect
ST_Contains = _sqlfunc.ST_Contains
ST_ConvexHull = _sqlfunc.ST_ConvexHull
ST_Crosses = _sqlfunc.ST_Crosses
ST_Difference = _sqlfunc.ST_Difference
ST_Dimension = _sqlfunc.ST_Dimension
ST_Disjoint = _sqlfunc.ST_Disjoint
ST_Distance = _sqlfunc.ST_Distance
ST_Distance_Sphere = _sqlfunc.ST_Distance_Sphere
ST_EndPoint = _sqlfunc.ST_EndPoint
ST_Envelope = _sqlfunc.ST_Envelope
ST_Equals = _sqlfunc.ST_Equals
ST_ExteriorRing = _sqlfunc.ST_ExteriorRing
ST_FrechetDistance = _sqlfunc.ST_FrechetDistance
ST_GeoHash = _sqlfunc.ST_GeoHash
ST_GeomCollFromText = _sqlfunc.ST_GeomCollFromText
ST_GeomCollFromTxt = _sqlfunc.ST_GeomCollFromTxt
ST_GeomCollFromWKB = _sqlfunc.ST_GeomCollFromWKB
ST_GeomFromGeoJSON = _sqlfunc.ST_GeomFromGeoJSON
ST_GeomFromText = _sqlfunc.ST_GeomFromText
ST_GeomFromWKB = _sqlfunc.ST_GeomFromWKB
ST_GeometryCollectionFromText = _sqlfunc.ST_GeometryCollectionFromText
ST_GeometryCollectionFromWKB = _sqlfunc.ST_GeometryCollectionFromWKB
ST_GeometryFromText = _sqlfunc.ST_GeometryFromText
ST_GeometryFromWKB = _sqlfunc.ST_GeometryFromWKB
ST_GeometryN = _sqlfunc.ST_GeometryN
ST_GeometryType = _sqlfunc.ST_GeometryType
ST_HausdorffDistance = _sqlfunc.ST_HausdorffDistance
ST_InteriorRingN = _sqlfunc.ST_InteriorRingN
ST_Intersection = _sqlfunc.ST_Intersection
ST_Intersects = _sqlfunc.ST_Intersects
ST_IsClosed = _sqlfunc.ST_IsClosed
ST_IsEmpty = _sqlfunc.ST_IsEmpty
ST_IsSimple = _sqlfunc.ST_IsSimple
ST_IsValid = _sqlfunc.ST_IsValid
ST_LatFromGeoHash = _sqlfunc.ST_LatFromGeoHash
ST_Latitude = _sqlfunc.ST_Latitude
ST_Length = _sqlfunc.ST_Length
ST_LineFromText = _sqlfunc.ST_LineFromText
ST_LineFromWKB = _sqlfunc.ST_LineFromWKB
ST_LineInterpolatePoint = _sqlfunc.ST_LineInterpolatePoint
ST_LineInterpolatePoints = _sqlfunc.ST_LineInterpolatePoints
ST_LineStringFromText = _sqlfunc.ST_LineStringFromText
ST_LineStringFromWKB = _sqlfunc.ST_LineStringFromWKB
ST_LongFromGeoHash = _sqlfunc.ST_LongFromGeoHash
ST_Longitude = _sqlfunc.ST_Longitude
ST_MLineFromText = _sqlfunc.ST_MLineFromText
ST_MLineFromWKB = _sqlfunc.ST_MLineFromWKB
ST_MPointFromText = _sqlfunc.ST_MPointFromText
ST_MPointFromWKB = _sqlfunc.ST_MPointFromWKB
ST_MPolyFromText = _sqlfunc.ST_MPolyFromText
ST_MPolyFromWKB = _sqlfunc.ST_MPolyFromWKB
ST_MakeEnvelope = _sqlfunc.ST_MakeEnvelope
ST_MultiLineStringFromText = _sqlfunc.ST_MultiLineStringFromText
ST_MultiLineStringFromWKB = _sqlfunc.ST_MultiLineStringFromWKB
ST_MultiPointFromText = _sqlfunc.ST_MultiPointFromText
ST_MultiPointFromWKB = _sqlfunc.ST_MultiPointFromWKB
ST_MultiPolygonFromText = _sqlfunc.ST_MultiPolygonFromText
ST_MultiPolygonFromWKB = _sqlfunc.ST_MultiPolygonFromWKB
ST_NumGeometries = _sqlfunc.ST_NumGeometries
ST_NumInteriorRing = _sqlfunc.ST_NumInteriorRing
ST_NumInteriorRings = _sqlfunc.ST_NumInteriorRings
ST_NumPoints = _sqlfunc.ST_NumPoints
ST_Overlaps = _sqlfunc.ST_Overlaps
ST_PointAtDistance = _sqlfunc.ST_PointAtDistance
ST_PointFromGeoHash = _sqlfunc.ST_PointFromGeoHash
ST_PointFromText = _sqlfunc.ST_PointFromText
ST_PointFromWKB = _sqlfunc.ST_PointFromWKB
ST_PointN = _sqlfunc.ST_PointN
ST_PolyFromText = _sqlfunc.ST_PolyFromText
ST_PolyFromWKB = _sqlfunc.ST_PolyFromWKB
ST_PolygonFromText = _sqlfunc.ST_PolygonFromText
ST_PolygonFromWKB = _sqlfunc.ST_PolygonFromWKB
ST_SRID = _sqlfunc.ST_SRID
ST_Simplify = _sqlfunc.ST_Simplify
ST_StartPoint = _sqlfunc.ST_StartPoint
ST_SwapXY = _sqlfunc.ST_SwapXY
ST_SymDifference = _sqlfunc.ST_SymDifference
ST_Touches = _sqlfunc.ST_Touches
ST_Transform = _sqlfunc.ST_Transform
ST_Union = _sqlfunc.ST_Union
ST_Validate = _sqlfunc.ST_Validate
ST_Within = _sqlfunc.ST_Within
ST_X = _sqlfunc.ST_X
ST_Y = _sqlfunc.ST_Y
SUBDATE = _sqlfunc.SUBDATE
SUBSTR = _sqlfunc.SUBSTR
SUBSTRING = _sqlfunc.SUBSTRING
SUBSTRING_INDEX = _sqlfunc.SUBSTRING_INDEX
SUBTIME = _sqlfunc.SUBTIME
SUM = _sqlfunc.SUM
SYSDATE = _sqlfunc.SYSDATE
SYSTEM_USER = _sqlfunc.SYSTEM_USER
TAN = _sqlfunc.TAN
TIME = _sqlfunc.TIME
TIMEDIFF = _sqlfunc.TIMEDIFF
TIMESTAMP = _sqlfunc.TIMESTAMP
TIMESTAMPADD = _sqlfunc.TIMESTAMPADD
TIMESTAMPDIFF = _sqlfunc.TIMESTAMPDIFF
TIME_FORMAT = _sqlfunc.TIME_FORMAT
TIME_TO_SEC = _sqlfunc.TIME_TO_SEC
TO_BASE64 = _sqlfunc.TO_BASE64
TO_DAYS = _sqlfunc.TO_DAYS
TO_SECONDS = _sqlfunc.TO_SECONDS
TO_VECTOR = _sqlfunc.TO_VECTOR
TRIM = _sqlfunc.TRIM
TRUNCATE = _sqlfunc.TRUNCATE
UCASE = _sqlfunc.UCASE
UNCOMPRESS = _sqlfunc.UNCOMPRESS
UNCOMPRESSED_LENGTH = _sqlfunc.UNCOMPRESSED_LENGTH
UNHEX = _sqlfunc.UNHEX
UNIX_TIMESTAMP = _sqlfunc.UNIX_TIMESTAMP
UPPER = _sqlfunc.UPPER
USER = _sqlfunc.USER
UTC_DATE = _sqlfunc.UTC_DATE
UTC_TIME = _sqlfunc.UTC_TIME
UTC_TIMESTAMP = _sqlfunc.UTC_TIMESTAMP
UUID = _sqlfunc.UUID
UUID_SHORT = _sqlfunc.UUID_SHORT
UUID_TO_BIN = _sqlfunc.UUID_TO_BIN
VARIANCE = _sqlfunc.VARIANCE
VAR_POP = _sqlfunc.VAR_POP
VAR_SAMP = _sqlfunc.VAR_SAMP
VECTOR_DIM = _sqlfunc.VECTOR_DIM
VECTOR_TO_STRING = _sqlfunc.VECTOR_TO_STRING
VERSION = _sqlfunc.VERSION
WEEK = _sqlfunc.WEEK
WEEKDAY = _sqlfunc.WEEKDAY
WEEKOFYEAR = _sqlfunc.WEEKOFYEAR
YEAR = _sqlfunc.YEAR
YEARWEEK = _sqlfunc.YEARWEEK

__all__: list[str] = [
    'ABS', 'ACOS', 'ADDDATE', 'ADDTIME', 'AES_DECRYPT', 'AES_ENCRYPT', 'ANY_VALUE',
    'ASCII', 'ASIN', 'ATAN', 'ATAN2', 'AVG', 'BIN', 'BIN_TO_UUID',
    'BIT_AND', 'BIT_COUNT', 'BIT_LENGTH', 'BIT_OR', 'BIT_XOR', 'CAST', 'CEIL',
    'CEILING', 'CHAR', 'CHARACTER_LENGTH', 'CHARSET', 'CHAR_LENGTH', 'COALESCE', 'COERCIBILITY',
    'COLLATION', 'COMPRESS', 'CONCAT', 'CONCAT_WS', 'CONNECTION_ID', 'CONV', 'CONVERT',
    'CONVERT_TZ', 'COS', 'COT', 'COUNT', 'CRC32', 'CUME_DIST', 'CURDATE',
    'CURRENT_DATE', 'CURRENT_ROLE', 'CURRENT_TIME', 'CURRENT_TIMESTAMP', 'CURRENT_USER', 'CURTIME', 'DATABASE',
    'DATE', 'DATEDIFF', 'DATE_ADD', 'DATE_FORMAT', 'DATE_SUB', 'DAY', 'DAYNAME',
    'DAYOFMONTH', 'DAYOFWEEK', 'DAYOFYEAR', 'DEGREES', 'DENSE_RANK', 'ELT', 'EXP',
    'EXPORT_SET', 'EXTRACT', 'FIELD', 'FIND_IN_SET', 'FIRST_VALUE', 'FLOOR', 'FORMAT',
    'FROM_BASE64', 'FROM_DAYS', 'FROM_UNIXTIME', 'FROM_VECTOR', 'GET_FORMAT', 'GREATEST', 'GROUPING',
    'GROUP_CONCAT', 'GeomCollection', 'GeometryCollection', 'HEX', 'HOUR', 'ICU_VERSION', 'IF',
    'IFNULL', 'INET6_ATON', 'INET6_NTOA', 'INET_ATON', 'INET_NTOA', 'INSERT', 'INSTR',
    'INTERVAL', 'ISNULL', 'IS_IPV4', 'IS_IPV4_COMPAT', 'IS_IPV4_MAPPED', 'IS_IPV6', 'IS_UUID',
    'JSON_ARRAY', 'JSON_ARRAYAGG', 'JSON_ARRAY_APPEND', 'JSON_ARRAY_INSERT', 'JSON_CONTAINS', 'JSON_CONTAINS_PATH', 'JSON_DEPTH',
    'JSON_EXTRACT', 'JSON_INSERT', 'JSON_KEYS', 'JSON_LENGTH', 'JSON_MERGE_PATCH', 'JSON_MERGE_PRESERVE', 'JSON_OBJECT',
    'JSON_OBJECTAGG', 'JSON_OVERLAPS', 'JSON_PRETTY', 'JSON_QUOTE', 'JSON_REMOVE', 'JSON_REPLACE', 'JSON_SCHEMA_VALID',
    'JSON_SCHEMA_VALIDATION_REPORT', 'JSON_SEARCH', 'JSON_SET', 'JSON_STORAGE_FREE', 'JSON_STORAGE_SIZE', 'JSON_TYPE', 'JSON_UNQUOTE',
    'JSON_VALID', 'JSON_VALUE', 'LAG', 'LAST_DAY', 'LAST_INSERT_ID', 'LAST_VALUE', 'LCASE',
    'LEAD', 'LEAST', 'LEFT', 'LENGTH', 'LN', 'LOCALTIME', 'LOCALTIMESTAMP',
    'LOCATE', 'LOG', 'LOG10', 'LOG2', 'LOWER', 'LPAD', 'LTRIM',
    'LineString', 'MAKEDATE', 'MAKETIME', 'MAKE_SET', 'MAX', 'MBRContains', 'MBRCoveredBy',
    'MBRCovers', 'MBRDisjoint', 'MBREquals', 'MBRIntersects', 'MBROverlaps', 'MBRTouches', 'MBRWithin',
    'MICROSECOND', 'MID', 'MIN', 'MINUTE', 'MOD', 'MONTH', 'MONTHNAME',
    'MultiLineString', 'MultiPoint', 'MultiPolygon', 'NOW', 'NTH_VALUE', 'NTILE', 'NULLIF',
    'OCT', 'OCTET_LENGTH', 'ORD', 'PERCENT_RANK', 'PERIOD_ADD', 'PERIOD_DIFF', 'PI',
    'POSITION', 'POW', 'POWER', 'Point', 'Polygon', 'QUARTER', 'QUOTE',
    'RADIANS', 'RAND', 'RANDOM_BYTES', 'RANK', 'REGEXP_INSTR', 'REGEXP_LIKE', 'REGEXP_REPLACE',
    'REGEXP_SUBSTR', 'REPEAT', 'REPLACE', 'REVERSE', 'RIGHT', 'ROUND', 'ROW_COUNT',
    'ROW_NUMBER', 'RPAD', 'RTRIM', 'SCHEMA', 'SECOND', 'SEC_TO_TIME', 'SESSION_USER',
    'SHA2', 'SIGN', 'SIN', 'SOUNDEX', 'SPACE', 'SQRT', 'STD',
    'STDDEV', 'STDDEV_POP', 'STDDEV_SAMP', 'STRCMP', 'STRING_TO_VECTOR', 'STR_TO_DATE', 'ST_Area',
    'ST_AsBinary', 'ST_AsGeoJSON', 'ST_AsText', 'ST_AsWKB', 'ST_AsWKT', 'ST_Buffer', 'ST_Buffer_Strategy',
    'ST_Centroid', 'ST_Collect', 'ST_Contains', 'ST_ConvexHull', 'ST_Crosses', 'ST_Difference', 'ST_Dimension',
    'ST_Disjoint', 'ST_Distance', 'ST_Distance_Sphere', 'ST_EndPoint', 'ST_Envelope', 'ST_Equals', 'ST_ExteriorRing',
    'ST_FrechetDistance', 'ST_GeoHash', 'ST_GeomCollFromText', 'ST_GeomCollFromTxt', 'ST_GeomCollFromWKB', 'ST_GeomFromGeoJSON', 'ST_GeomFromText',
    'ST_GeomFromWKB', 'ST_GeometryCollectionFromText', 'ST_GeometryCollectionFromWKB', 'ST_GeometryFromText', 'ST_GeometryFromWKB', 'ST_GeometryN', 'ST_GeometryType',
    'ST_HausdorffDistance', 'ST_InteriorRingN', 'ST_Intersection', 'ST_Intersects', 'ST_IsClosed', 'ST_IsEmpty', 'ST_IsSimple',
    'ST_IsValid', 'ST_LatFromGeoHash', 'ST_Latitude', 'ST_Length', 'ST_LineFromText', 'ST_LineFromWKB', 'ST_LineInterpolatePoint',
    'ST_LineInterpolatePoints', 'ST_LineStringFromText', 'ST_LineStringFromWKB', 'ST_LongFromGeoHash', 'ST_Longitude', 'ST_MLineFromText', 'ST_MLineFromWKB',
    'ST_MPointFromText', 'ST_MPointFromWKB', 'ST_MPolyFromText', 'ST_MPolyFromWKB', 'ST_MakeEnvelope', 'ST_MultiLineStringFromText', 'ST_MultiLineStringFromWKB',
    'ST_MultiPointFromText', 'ST_MultiPointFromWKB', 'ST_MultiPolygonFromText', 'ST_MultiPolygonFromWKB', 'ST_NumGeometries', 'ST_NumInteriorRing', 'ST_NumInteriorRings',
    'ST_NumPoints', 'ST_Overlaps', 'ST_PointAtDistance', 'ST_PointFromGeoHash', 'ST_PointFromText', 'ST_PointFromWKB', 'ST_PointN',
    'ST_PolyFromText', 'ST_PolyFromWKB', 'ST_PolygonFromText', 'ST_PolygonFromWKB', 'ST_SRID', 'ST_Simplify', 'ST_StartPoint',
    'ST_SwapXY', 'ST_SymDifference', 'ST_Touches', 'ST_Transform', 'ST_Union', 'ST_Validate', 'ST_Within',
    'ST_X', 'ST_Y', 'SUBDATE', 'SUBSTR', 'SUBSTRING', 'SUBSTRING_INDEX', 'SUBTIME',
    'SUM', 'SYSDATE', 'SYSTEM_USER', 'TAN', 'TIME', 'TIMEDIFF', 'TIMESTAMP',
    'TIMESTAMPADD', 'TIMESTAMPDIFF', 'TIME_FORMAT', 'TIME_TO_SEC', 'TO_BASE64', 'TO_DAYS', 'TO_SECONDS',
    'TO_VECTOR', 'TRIM', 'TRUNCATE', 'UCASE', 'UNCOMPRESS', 'UNCOMPRESSED_LENGTH', 'UNHEX',
    'UNIX_TIMESTAMP', 'UPPER', 'USER', 'UTC_DATE', 'UTC_TIME', 'UTC_TIMESTAMP', 'UUID',
    'UUID_SHORT', 'UUID_TO_BIN', 'VARIANCE', 'VAR_POP', 'VAR_SAMP', 'VECTOR_DIM', 'VECTOR_TO_STRING',
    'VERSION', 'WEEK', 'WEEKDAY', 'WEEKOFYEAR', 'YEAR', 'YEARWEEK',
]
