# Published SQLAero 0.1.0a1 verification

On 2026-10-04 the user published prerelease v0.1.0a1 at commit
344cf4d75d0cef8b3badfa238ba0465f66684b58. The release-triggered build, tests
and OIDC upload succeeded: https://github.com/AresJef/SQLAero/actions/runs/37185072839.
The real project is https://pypi.org/project/SQLAero/0.1.0a1/.

Published file: sqlaero-0.1.0a1-cp313-cp313-macosx_26_0_arm64.whl.
SHA256: cb769f2a5b2236d122e88c50883b4fb06bd4782d09404fa343dbfc48c12a9673.
Official PyPI JSON, downloaded bytes, installation report and attestation subject
matched this digest. Public provenance identified AresJef/SQLAero, release.yml,
and environment pypi. Filename/internal WHEEL/METADATA and all 19 actual Mach-O
extensions passed the arm64, CPython 3.13 and minimum macOS 26.0 checks.

A fresh venv on macOS 27.0.1 arm64 / Python 3.13.3 installed SQLAero==0.1.0a1
from https://pypi.org/simple using --isolated, --only-binary=:all: and
--no-cache-dir. The actual download URL and SHA in pip's report matched PyPI.
pip check, import sqlaero, explicit utils/sqlfunc scripts and the pure
TestEscape/TestDecode runner passed. The pure runner blocks socket networking.
Cython and the old sqlcycli namespace were absent. No real database was used.
The binary runner's macOS26 tests and the independent macOS27 installation
are distinct evidence, not a general platform or database compatibility claim.

The compact manifest and offline output (line endings/trailing whitespace normalized) are retained in release-evidence/0.1.0a1/;
no credentials, binary artifacts or venvs are committed. The attempted nice
priority adjustment was denied by the host, but the short tests ran successfully.

Limits: database/connection/pool integration, alternate platforms/architectures,
other Python versions, optional authentication dependencies and all permitted
dependency combinations remain unverified. The PyPI account-management UI's
pending-to-active display was not directly rechecked because Chrome control timed
out; actual successful upload and public Trusted Publishing provenance were checked.

This records the published first release. Later local workflow changes are
not yet remotely CI-validated and do not change this immutable release/tag.
