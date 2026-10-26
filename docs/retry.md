# Explicit synchronous retry

Retry is an outer application policy. `execute`, `transaction` and connection
establishment never silently replay SQL or reconnect. Matching an error type
or code does not prove that replaying an unknown request is safe. The caller
owns new connections, transactions, repeatable inputs and external side effects.

```python
from sqlaero.errors import IntegrityError, OperationalError, MySQLErrorCode
from sqlaero.retry import Retry, RetryRule, retry

rules = (
    RetryRule(error_types=OperationalError,
              error_codes=MySQLErrorCode.ER_LOCK_DEADLOCK),
    RetryRule(error_types=IntegrityError),
)

@retry(*rules, max_attempts=3, wait_interval=0.05,
       wait_multiplier=2, max_wait_interval=1, wait_jitter=True)
def work(user_id: int) -> int:
    with open_connection() as connection, connection.transaction() as cursor:
        return cursor.execute('UPDATE users SET active=%s WHERE id=%s', (True, user_id))

# The block form replays only the explicit block. Configuration is reusable;
# each traversal and decorated invocation has its own count/backoff progress.
for attempt in Retry(*rules, max_attempts=3, wait_interval=0.05):
    with attempt, open_connection() as connection, connection.transaction() as cursor:
        cursor.execute('UPDATE users SET active=%s WHERE id=%s', (True, 7))
```

A rule combines its conditions with **AND**. Each list/tuple within a condition
matches any member; different rules combine with **OR**. Supplying only a type
or only a code is valid. At least one nonempty condition is required; explicit
empty lists/tuples are rejected even if another condition is present. Types
must be `Exception` subclasses; control-flow `BaseException` classes cannot be
configured. Codes are integers/IntEnum values, excluding bool, normalized once.
Code matching reads only a SQLAero primary's real code, not an external `code`
attribute, OSError.errno, cause/context/additional_errors or diagnostic text.

| Control | Default | Meaning |
| --- | --- | --- |
| `max_attempts` | `3` | Includes the first attempt; built-in int 1..2**64-1, or `None` for unlimited. |
| `wait_interval` | `1.0` | Finite nonnegative seconds; fractions and zero are valid. |
| `wait_multiplier` | `1.0` | Finite value >= 1; one is fixed delay. |
| `max_wait_interval` | `10.0` | Mandatory finite nonnegative cap, including on the first retry/fixed delay. |
| `wait_jitter` | `False` | Uniform actual delay between zero and its capped base. |

Numeric wait controls accept built-in int/float, excluding bool, and must be
safely representable. Jitter never feeds its random result into the next base.
For base=1, multiplier=2, cap=10 the retry bases are 1,2,4,8,10,10. Zero skips
waiting. There is no wait before the first attempt, after success or after the
last failure. Waiting releases the GIL and checks Python interruption at the
existing native control interval; scheduling can take longer than requested.

The final/unmatched/control-flow exception propagates as the same object, with
its own arguments, chain, traceback and notes. Retry adds no exhaustion wrapper,
notes or cross-attempt exception history, and does not materialize ErrorInfo for
matching. There is no `max_duration`: connection/operation timeouts constrain
each database command, not arbitrary user code or the entire retry loop.

`RetryRule` and `Retry` properties are read-only. An attempt is created only by
iteration and entered/exited once. Complete its `with` block before asking the
same traversal for another attempt; distinct traversals are independent.

The decorator is a thin typed Python call boundary using `functools.wraps`;
matching, count/backoff and bounded waiting are native. It preserves normal
method binding, introspection, ParamSpec parameters and the linked return type.
The shared Rust SQL core stores no user callable or Python exception history.
