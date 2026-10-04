# Dispatch comparison preparation

No performance winner has been selected. No heavy benchmark has run. Measurements wait for the user's exclusive window outside cyTimes work, and for correctness/security gates.

Current baseline `_dispatch_exact` obtains `type(value)` once and compares identities in an explicit Cython if chain; `_dispatch_subclass` handles only known builtin/calendar subclass categories. `_literal` executes registered ObjStr/RawText/SQLFunction/SQLInterval semantics first, preserving overriding `__str__`, bool/int order and datetime/date order. Decimal and ndarray exact-only behavior is retained; normal pandas/cytimes calendar subclasses enter the calendar slow path. Handler bodies are the same regardless of any candidate classifier. Current NumPy/pandas handlers still contain additional exact checks outside the primitive classifier, so the initial primitive experiment is not evidence about the entire type universe.

Generated-C inspection of latest `sqlaero_cython_kernel.c` shows `_dispatch_exact` uses `Py_TYPE(value)` (with an owned reference) and comparisons such as `typ == &PyBool_Type`, `typ == &PyInt_Type`, `typ == &PyFloat_Type`. Calendar and Decimal comparisons currently fetch module attributes, a cost to cache before interpreting classifier measurements. Cython translates handler integer equality chains separately; compiler optimization must be inspected in assembly for switch lowering. There is no Python dictionary-to-Python-callable dispatch in the baseline and no genuine C function-pointer table yet. A Python callable registry comparison must not be mislabeled a C function table.

Candidate implementations to compare with identical supported inputs, trust checks, handler functions, errors and outputs:

| Candidate | Required measured costs | Maintainability |
|---|---|---|
| Exact-type if chain | Type access, identity comparisons, branch prediction, subclass fallback | Auditable chain; consider reproducibly generating from a declaration table only if growth warrants it |
| Type ID plus C switch | Full type-to-ID acquisition plus switch, not switch alone | One type declaration plus handler; ID stability internal only |
| Genuine C function-pointer table | Type-to-ID plus indirect C call and error propagation, same function bodies | Compact table; explicitly typed C signatures and checked index bounds |
| Fixed hot path plus internal extension registry | Hot comparisons plus explicit cold lookup; no arbitrary `__str__` trust | Common path stable; extension types centralized in declaration/handler/tests |
| Python type dictionary diagnostic | Dictionary lookup and Python call overhead separately | Diagnostic control only, not evidence against genuine C tables |

The initial public `dispatch_code` is an experimental classification probe only. It excludes trusted expression handling and array/pandas handlers, so it is suitable for isolating primitive-classifier mechanics, not for claiming an end-to-end speedup. The registry is internal extensibility design, not a promised new public semantic extension API.

Prepared matrix:

- Per-type homogeneous repeats: native bool/int/float/str/bytes/None/Decimal/calendar types, each supported NumPy scalar dtype, container, array and pandas category, plus four shared bridge classes and all expression types.
- Realistic fixed mixtures: named workload distributions for query parameters, result values and dataframe cells, with transparently declared proportions. Never select a distribution after seeing a winner.
- Random high-entropy mixtures: seeded shuffled supported exact types, then a separate subclass-heavy/cold-hook mixture. Registration/trust semantics remain identical across variants.
- Growing type sets: 16, 32, 64, 128 declarations using approved inert synthetic types and identical handler work; report declaration growth, lookup/setup cost and mixed-order sensitivity. Synthetic semantics must not be merged into production support.
- Separation: classifier-only loop in native code, full `escape` with Python boundary/output allocations, and classifier+identical no-op native handler. Report type-ID acquisition separately but include it in totals.

Report per-type median/p95, sample counts, randomized candidate order, confidence intervals/noise floor, warmed/cold distinctions, source hashes, CPython/Cython/clang versions, optimization flags and architecture. Build log currently records clang `-O3 -DNDEBUG`; generated-C diagnostics are unrelated unreachable-code/fallthrough warnings. Correctness requires every candidate to share exact/subclass acceptance, ObjStr priority, flags/shapes and error policy before any speed comparison. If no candidate beats the identity chain consistently, retain the chain. Algorithm, encoding, handler allocation, array copying or binding changes must be held constant and cannot be attributed to dispatch.
