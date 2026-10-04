# Dispatch design and controlled comparison

Implemented candidates in `src/dispatch.rs` all feed the identical SQL scalar handlers in `src/lib.rs`. Explicit ObjStr/RawText/SQLFunction/SQLInterval trust checks always precede ordinary type classification. Ordinary subclasses use reviewed fallback handlers and remain escaped. `configure_types` and `_set_dispatch_strategy` are internal bridge/research hooks, not a proposed public extensibility API.

| Candidate | Actual implementation | Cost included in future measurements |
| --- | --- | --- |
| Exact type cascade (`cascade`) | Get type pointer; compare pinned builtin identities in fixed order | Pointer acquisition, each comparison, common handler call |
| Type ID plus match (`type_id`) | Get type pointer; hash lookup gives `Kind` enum; match shared handler | Hashing and lookup to compute ID, enum dispatch and handler |
| Function table (`function_table`) | Get type pointer; hash lookup calls classifier function pointer returning same `Kind` | Hashing, indirect classifier call, enum dispatch and handler |
| Fixed hot path plus registry (`hot_registry`) | Five exact common builtin comparisons then lookup; reviewed extension catalogue handles uncommon types | Hot comparisons, miss lookup, extension subtype checks and same handler |

The default is a provisional implementation choice, not a measured fastest strategy. No speed, allocation, SIMD, GIL-release, or throughput claims are made. Numeric native-array loops use their own dtype dispatch after metadata validation and do not repeat Python dispatch per element.

`KNOWN_TYPES` centralizes reviewed extension declarations and caches class objects once at import. A new type requires (1) its explicit declaration and exact/subclass acceptance decision, (2) a dedicated handler using existing writers, (3) normal, rejected, shape, error, trust and safety vectors, and (4) a support-matrix row. New type support never automatically trusts `__str__` or arbitrary iterable/mapping protocols. Additional trusted SQL class families are an explicit semantic policy decision.

Future comparisons must use the same handlers and complete Python-call boundary, not an isolated enum microbenchmark. Grow the type population from common builtins to all existing families, then prospective declared families. Fixtures must include single-type, clustered mixed, randomized mixed with fixed seeds, mostly-common with rare types, subclass and rejected objects, and trust hooks. Include type-to-ID computation, extension lookup, output construction and callbacks. Warm each candidate equally, interleave execution order, report distributions and type-population sensitivity. Test output value, shape, Python result type and exception class/cause parity before timing. The current nine-test correctness suite verifies all four candidates on the same values.

Heavy performance comparisons remain deferred to the authorized exclusive window so cyTimes work is not disturbed. This experiment supplies runnable candidates without claiming a dispatch winner or expanding the public API.
