# Errors and immutable diagnostics

`SQLAeroError` is the root for SQLAero failures. Its read-only `message`, `code`
and `is_server_error` are independent constructor values. A manual exception can
omit the code/source; actual driver failures fill them from their real origin.
`args` contains only the supplied message. `OperationalTimeoutError` inherits
`OperationalError`; it does not also inherit Python's `TimeoutError`.
`SQLAeroWarning` is a separate warning category.

```python
from sqlaero.errors import IntegrityError, MySQLErrorCode

try:
    cursor.execute("INSERT INTO users(id) VALUES (%s)", (user_id,))
except IntegrityError as error:
    if error.code == MySQLErrorCode.ER_DUP_ENTRY:
        handle_duplicate()
```

The maintained client/server catalogs, enum values, provenance and unknown-code
SQLSTATE fallback are described in [error-codes.md](error-codes.md). The server's
code and source are never inferred from human message text. Known codes have
priority over SQLSTATE fallback. Ordinary successful-response warnings are not
silently turned into exceptions.

## Reading diagnostics

`error.error_info` returns an immutable `ErrorInfo`; it is not an exception and
cannot be raised. Its fields appear in this order:

| Field | Meaning |
| --- | --- |
| `stage` | Actual recorded handling boundary, or `None`. |
| `exception_type` | Actual Python exception class, or `None` for native source nodes. |
| `native_type` | Reliably known Rust type name; diagnostic text, not a stable matching key. |
| `message` | Frozen original node message. |
| `code` | This node's actual SQLAero code, or `None`. |
| `os_errno` | Actual OS error number of this node alone, or `None`. |
| `is_server_error` | This SQLAero node's source; external/native nodes may have `None`. |
| `cause` | Actual explicit/native cause snapshot. |
| `context` | Actual independent Python context, including suppressed context. |
| `suppress_context` | Original Python implicit-context display suppression. |
| `traceback` | Lazily formatted captured Python stack and applicable chain, or `None`. |
| `notes` | Frozen ordered tuple of notes. |
| `additional_errors` | Ordered tuple of independent later failures. |

```python
info = error.error_info
for later in info.additional_errors:
    print(later.stage, later.exception_type, later.code, later.message)
print(info.traceback)
```

Cause and context can point to the same node, different nodes or a genuine cycle.
Snapshots preserve those relationships; text formatting terminates at a cycle
and marks it. Causes/context are not automatically additional independent
failures. Native I/O causes retain their actual OS number without fabricating
Python exception classes or traceback frames.

Repeated reads reuse the current snapshot. The first read captures the primary
Python stack/chain/notes at that point. A later library-recorded rollback,
cleanup failure or relevant fact invalidates only the current cache: the next
read creates an updated snapshot, while any earlier saved snapshot stays
unchanged. User mutations to an original exception are not monitored.
Secondary failure locations are captured when those failures actually occur.
Detailed traceback formatting and the additional-errors tuple are lazy/cached.

Snapshots retain source locations via `traceback.StackSummary`, with
`capture_locals=False`; they do not retain original exception instances, live
frames, locals, Cursors or result buffers. Python GC handles genuine cyclic
snapshot edges. No diagnostic read sends a database request.

## One propagated primary error

A business exception remains the same primary object if rollback or cleanup
also fails. A COMMIT failure is primary after a successful body; Cursor cleanup
must not overwrite it. Independent later failures are captured in
`error.error_info.additional_errors` for a SQLAero primary. A standard/user
primary keeps its identity, args and chain and receives only necessary notes;
it does not receive custom SQLAero attributes.

Confirmed commit/rollback, unattempted work and unknown outcomes are distinct
facts. Diagnostics do not replay SQL or claim that a confirmed batch prefix
means a committed transaction. After an entered execute fails, its aggregate
Cursor counters are unknown; confirmed prefix facts remain in diagnostics.

`str(info)` is a one-line escaped summary; `repr(info)` is compact and does not
recursively dump the graph or print object addresses. Notes appear in a full
Python traceback rather than automatically appearing in `str(error)`.

Server ERR text uses a response encoding only when that fact is confirmed.
Arbitrary SQL can change the result encoding and then fail without returning
metadata that identifies the new encoding. In that case the received bytes are
shown as lossless ASCII byte escapes with an explicit unconfirmed-encoding note;
being decodable by the old codec does not prove that codec is still correct.
The real class, code, source and SQLSTATE diagnostic are preserved. No extra
query or guessed encoding is used to repair error text.

Both frozen MySQL lines can truncate converted UTF-16/UCS-2/UTF-32 error text at
a NUL byte. Missing server bytes are never reconstructed, including when the
received message is empty. SQLSTATE remains a diagnostic note rather than an
additional public exception attribute.
