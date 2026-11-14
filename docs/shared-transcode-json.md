# Shared transcode and JSON input guide

This describes the shared-transcode development candidate. Its own source,
installed-package, concurrency, CI and performance reports determine validation
status. It is not a claim that a historical published Beta has these additions.

## Two public exits

`json_dumps(obj, *, allow_nan=True) -> bytes` returns one complete compact UTF-8
JSON document. It does not use a Connection, a SQL mode, a SQL timeout or a
MySQL packet quota. Actual memory capacity and cooperative signal checks still
apply. No partial document is returned on failure.

```python
from decimal import Decimal
from sqlaero import JSON, json_dumps

data = {"id": 7, "price": Decimal("12.500"), "tags": ["ab", None]}
assert json_dumps(data) == b'{"id":7,"price":"12.500","tags":["ab",null]}'

wrapped = JSON(data)
assert wrapped.data is data
data["id"] = 8
# The next Escape call reads the current object, not a cached JSON document.
```

`JSON(obj, *, allow_nan=True)` retains a read-only reference called `data` and
a read-only `allow_nan` option. It serializes lazily at each Escape call, then
the MySQL target encodes and quotes the document as **one text value** using
the actual SQL context. It is neither a raw SQL fragment nor a native MySQL
JSON parameter. This works through ordinary text and prepared execution.
It does not automatically serialize every ordinary dict, list or str parameter.

With an existing Connection and a table containing a JSON column:

```python
with connection.cursor() as cursor:
    cursor.execute("INSERT INTO documents(document) VALUES (%s)", (JSON(data),))
    cursor.execute("SELECT %s", (JSON(data),))  # A JSON document as SQL text.
    cursor.execute("SELECT CAST(%s AS JSON)", (JSON(data),))  # Server JSON expression.
```

MySQL decides whether the SQL expression or column accepts that document and
which numeric values it can preserve. Client serialization of a large integer
does not guarantee that the server's native JSON number representation can
preserve it. JSON text inside a document is not rewritten as a SQL temporal
parameter. Passing the bytes returned by `json_dumps` as an ordinary parameter
uses the existing binary-value contract; use `JSON` when a text parameter is
intended. Neither exit accepts nested JSON wrappers; pass `wrapped.data`
explicitly when composing another document.

## Values and structure

| Source family | JSON policy |
| --- | --- |
| None and real provider missing values | null, including with allow_nan=False |
| bool | true/false; not an object key |
| Approved integers | Exact JSON number, without MySQL's 65-digit SQL policy |
| Approved float16/32/64 and Python float | Actual binary value promoted without discarding precision; preserve negative zero |
| Finite Decimal | Representation-preserving JSON string, including scale and negative zero |
| str and approved text storage | Unicode JSON string; no normalization or guessed byte decoding |
| bytes and binary schemas | Rejected; no implicit UTF-8 or Base64 conversion |
| Approved date/time/timestamp/duration | Documented temporal string policy, described below |
| Ordinary sequences and 1D value series | JSON value array; Series index/name are not emitted |
| Named records, structured records and Struct values | Field-name object, recursively |
| DataFrame, Arrow Table/RecordBatch and qualified table sources | Array of field-name objects, one per row |
| Ordinary N-dimensional arrays | Preserve array axes; not a SQL row/placeholder inference |
| dict | Complete key/value object |
| dict keys/values/items views | Key array/value array/array of key-value pairs |
| UUID and plain Enum/Flag | Canonical UUID text / approved actual member value |

Real subclasses use supported actual base storage, not replacement `__int__`,
`__float__`, `__str__`, iteration or display methods. A source library must
actually supply a qualified subclass instance; a constructor returning its
base type does not demonstrate subclass support.

Object keys use the approved text, integer and UUID families. Bool, float,
Decimal, tuple, temporal and missing keys are not implicitly converted.
Final full key names must be unique; a collision such as integer 1 and text
"1" raises ValueError. Names remain case-sensitive and are not normalized.
NUL suffixes remain significant: "a\0x" and "a\0y" are distinct names and are
escaped as JSON Unicode controls.

Parent nulls and real masks suppress their payloads, not logical positions.
Each dictionary chunk uses its own dictionary and emits actual values.
Chunks do not introduce extra array axes. Empty sources still undergo the
applicable schema admission; an empty binary schema does not gain JSON support.
Repeated noncyclic references are allowed. Cycles and more than the approved
64 actual output container levels are rejected; transparent source wrappers
do not invent output levels.

## Nonfinite and temporal policies

`allow_nan` requires an exact bool. With True, valid NaN, positive/negative
Infinity and Decimal nonfinite values become null. False raises ValueError
for those valid nonfinite numbers. This does not reject genuine missing values
such as None, NaT or actual masks. Finite unsupported temporal units are not
made valid by the missing-value rule.

JSON retains full approved source precision: ordinary microsecond fractions
use six digits, real submicrosecond temporal fractions use nine, and zero
fractions are omitted. UTC uses `+00:00`. An aware datetime preserves its
original clock and actual offset; an aware time retains its effective offset.
Wide years and offsets containing seconds/fractions are documented extensions,
not a promise of strict RFC3339 formatting.

Fixed durations use one exact duration policy across providers, with a whole
value sign and trimmed fractional trailing zeroes. Calendar year/month
timedelta64 values remain calendar-period strings; they are not claimed equal
to fixed elapsed seconds. A struct_time emits its eleven original fields.

MySQL has a separate target policy. Text and prepared temporal parameters
truncate locally to microseconds without rounding. Negative durations truncate
toward zero; negative epochs first decompose into calendar seconds. Aware
datetimes use the actual UTC session precondition, while standalone time values
keep their clock and ignore tzinfo. MySQL continues its established average
conversion for calendar year/month durations. The server decides target column
ranges and other SQL validity; ordinary text and JSON document contents are not
parsed and truncated by the client.

## Errors, ownership and GIL/NoGIL

Unsupported types or schemas raise TypeError. Cycles, output depth, nonfinite
strict mode and final key collisions raise the applicable ValueError. Actual
Unicode surrogate codepoints raise UnicodeEncodeError, even when two happen
to look like a surrogate pair; an ordinary Unicode emoji remains supported.
Original provider/protocol and signal exceptions are preserved with common
source coordinates. A MySQL request capacity error retains its typed client
resource error and code rather than becoming a content error.

Both interpreter modes use the same semantics. Keep mutable inputs and external
storage stable during a call, or synchronize all readers and writers. A Python
reference, read-only view, library-only lock or preliminary copy does not make
rogue NumPy writes, resize, unmapping or file truncation safe. Containers do not
provide a whole-tree atomic snapshot.

Long native traversal, copying, algorithms and writing check original control
cooperatively. Necessary public Decimal snapshots, complete field-name reads
and qualified CPU C Data source/ownership handoffs may internally be atomic;
the library checks around those calls and does not promise instantaneous
preemption inside the provider. This exception does not permit whole Python
value materialization or waive bounds/ownership checks in native work.

Polars 2.1.0 cannot safely export certain NUL-containing Series, table and
nested Struct names. Those unsafe sources are explicitly rejected before
export, including on MySQL or empty paths that must perform that export.
Qualified PyArrow complete field names retain their NUL suffixes. Text values
containing NUL are a different case and keep their normal value policy.
