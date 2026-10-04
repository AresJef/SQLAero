# Visible behavior and safety decisions

Accepted safe normal-value contract: original return types, many/itemize precedence, public four-class inheritance and hook priority, decimal precision, dictionary values, SQL zero-date fallback and connection codec transport. No production implementation replaced.

Safety corrections tracked separately: reject BIT >8 bytes, negative or >uint64 integers rather than truncation; reject integer overflow and malformed numeric tokens; add valid exponent float decoding; validate array layout/endian/alignment; NaT handling and checked temporal scaling; bounded recursion/cycle rejection. Unsafe arithmetic is never a compatibility mode.

Requires further coordinator decision: negative-zero/exponent textual formatting, strict malformed temporal separators/trailing characters, arbitrary bigint SQL literals vs field numeric decode limits, tuple parentheses on mixed nested containers, all zero-dimensional/empty-2D cases, JSON dependency-specific type expansion, SQL mode and non-UTF8 charset strategy. Do not silently declare these equivalent to frozen baseline.

Initial experimental kernels target original standalone backslash-escaping API. Since it has no sql_mode/charset argument, advertise a backslash-enabled mode premise; production integration needs connection-aware policy or rejection of unsupported modes before selecting a kernel. No promise of universal SQL mode/charset safety.

NumPy calendar/duration dtype multipliers (e.g. datetime64[3D]) are a known original semantic defect: SQL baseline ignores the multiplier. Preserve that safe normal behavior for now; applying the multiplier is a separate visible correction awaiting user decision, while overflow checks remain mandatory. Do not claim the current output matches NumPy semantic dates. JSON multiplier behavior is not safely frozen: a worker isolated probe exited SIGTRAP without sufficient output to attribute a cause; do not repeat it or call it an orjson vulnerability. Keep this frontier explicitly unverified.

BIT accepted domain remains bytes/bytearray/memoryview/np.bytes_, Python int (including bool), and exact supported NumPy integer classes. Preserve original bool SQL text True/False; reject NumPy bool_ and arbitrary __index__ objects, rather than broadening the normal type domain. Overlong/negative/above-uint64 rejection remains an explicit safety/range behavior change.
