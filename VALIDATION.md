# SQLAero local preparation and validation

Version: 0.1.0a1. Distribution: SQLAero. Import namespace: sqlaero.
Baseline: SQLCyCli 2.3.1, aa666c6a179ab020cb8a3616734ca284623c45f2.
Validated locally on 2026-10-04.

## Change scope and provenance

74 tracked source/test/data files were checked byte-for-byte against the
baseline after mechanical name replacement. No runtime implementation was
refactored. setup.cfg changes name/version, removes the unconfirmed new
repository URL, and disables in-place extension output. README explains the
alpha provenance, migration, optional authentication dependencies and test
limits. The original LICENSE is retained verbatim. The aiomysql upstream MIT
license carries the same PyMySQL contributor notice already included here.
Legacy token publishing workflow, test_data/my.cnf, environments and prior
build output were excluded. Original repository/environment were read-only.

## Completed verification

- Host: macOS 27.0.1 arm64; Python 3.13.3; Apple clang 21.0.0.
- Build: isolated PEP 517 sdist, followed by wheel from extracted sdist;
  serial native compilation at low priority in /tmp/sqlaero-validation.
- Wheel tag: cp313-cp313-macosx_26_0_arm64; 19 compiled extensions.
  This tag reflects the interpreter/toolchain target, not a macOS 26 test.
- Fresh venv install from the actual wheel and pip check: passed.
- Runtime set: cytimes 3.1.0, NumPy 2.5.3, pandas 2.3.3, orjson 3.12.0,
  typing-extensions 4.16.0; Cython 3.2.1; setuptools 84.0.0.
  Published binary dependencies were used, never a local cyTimes checkout.
- 37 Python files parsed successfully. Four pre-existing docstring escape
  SyntaxWarnings and duplicate linker rpath warnings remain unchanged.
- Explicit test_utils.py and test_sqlfunc.py script entry points: passed
  against installed wheel using python -I -B. Initial direct source-path
  invocation shadowed the wheel; README now documents isolated mode.
- Explicit TestEscape.test_all and TestDecode.test_all: passed. The test
  module was loaded without invoking __main__; a Python audit hook rejected
  socket connection, DNS and bind events. No database was used. Existing
  tests additionally required pendulum 3.2.0 in the temporary venv.
- Independent downstream Cython extension cimported ObjStr and Connection,
  compiled and returned sqlaero.transcode / sqlaero.connection: passed.
- Wheel metadata, .pxd/.pyi inclusion, all 19 extensions, original LICENSE,
  absence of sqlcycli namespace and excluded config/workflow: passed.
- twine check on sdist and wheel: passed.
- Coinstalled official SQLCyCli 2.3.1 wheel: both imports/escape calls passed;
  distribution RECORD paths had no overlap. After uninstalling SQLAero,
  SQLCyCli still worked. Reinstalled SQLAero then uninstalled SQLCyCli:
  SQLAero still worked, old import was absent, and pip check passed.

## Limits and remaining release work

This does not validate Linux, Windows, x86_64, universal2, other CPython
versions, minimum dependency bounds, or optional authentication packages.
Sync/async connection and pool integration tests were not run. Do not claim
database regressions passed or publish old benchmark figures as alpha results.

Before the jointly operated GitHub/PyPI step: confirm repository owner/name,
support matrix and database regression expectations. Configure Trusted
Publishing separately, with the exact repository/workflow/environment mapping.
No remote repository, push, release, token setup or PyPI/TestPyPI upload was
performed. Pending publisher registration does not reserve a PyPI name.

Artifacts and SHA256SUMS are retained in dist/. Relevant logs are retained in
validation/; temporary build/test environments remain /tmp/sqlaero-validation.
