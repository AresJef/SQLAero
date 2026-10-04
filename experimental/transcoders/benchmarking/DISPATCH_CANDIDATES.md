# Reviewed dispatch candidates

No winner is preselected. Public production source and production defaults are unchanged. Each independent backend retains its prior experimental default. `_dispatch_strategies` and the selector are internal research controls, not a new public extension API.

| Backend | Fixed identity baseline | Meaningful alternatives | Actual mechanism |
|---|---|---|---|
| Cython | exactif (default) | hotcold, typeid_dict | Hot builtin pointer checks plus cold identity-ID dictionary, or complete primitive identity-ID dictionary. Type-address boxing, integer hashing/lookup and selector costs are inside the full public call. |
| Rust | cascade | type_id, hot_registry (prior default) | Cached type pointer comparisons; pointer-ID hash lookup to Kind then common handler match; common exact hot checks then reviewed cold registry. |

Cython dictionary candidates use CPython dictionary calls, not a native C handler function table or a proven C switch. Arbitrary type-object hash/equality hooks are avoided through identity-address keys. Rust removed its former function_table classifier: it duplicated the type_id lookup, added an indirect classifier callback and still entered the same handler match. This adds no useful independent handler-dispatch design. A genuine direct-handler table would require splitting handlers with uniform checked signatures and independently validating errors/ownership; both implementations defer that change rather than mislabel an extra indirection.

All candidates share the same handlers, trusted ObjStr/SQL-expression priority, known subclass policy, return shapes, flags and error behavior. Arrays/pandas/calendar fallback remains common; primitive classifier results cannot establish whole-type-universe performance. Scalar bool/int and datetime/date precedence remain explicit. Trusted hooks never expand to ordinary arbitrary __str__ objects. New types require the internal declaration/acceptance rule, shared handler, support matrix and exact/subclass/rejected/trust/shape tests described in each backend's DISPATCH_DESIGN.md.

Every candidate passed worker matrices404types,58JSON,492floats,20subclasses. Coordinator tests compare all419 public vectors and76 trusted/subclass/error/shape vectors for each strategy, including hostile metaclass hash/equality, JSON overrides, assertion hooks and invalid BIT domains. New Cython sanitizer run70passed,1explicit opt-in integration skipped; this remains current Mac arm64/Python3.13 evidence, not an exhaustive safety guarantee.

`dispatch_candidates` exposes the actual reviewed catalog. Strategy control setup is outside timed batches; type acquisition/ID lookup, Python adapter and full public call are inside timing. `run_dispatch_cases` uses the same attested child, shared lease, load gates and complete balanced-cycle rules as the mandatory separate old/Cython/Rust comparison. The first real round is deliberately limited to one str case and one three-party cycle; no dispatch timing winner will be selected from it.

Duration-based worker policy is predeclared: fixed calls per batch, at least the declared repetitions and minimum elapsed batch interval, with an explicit repetition cap and owned-child timeout. Every batch records total time, actual calls and normalized seconds per call; raw stderr is flushed so interrupted workers retain completed batches. Different backend repeat counts under the same duration rule are recorded, never treated as independent statistical observations. Whole-cycle intervals remain absent for one independent cycle.
