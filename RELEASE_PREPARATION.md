# First alpha publishing preparation

The user confirmed CPython 3.13 / macOS arm64 wheel-only publication.
setup.cfg now declares Python >=3.13,<3.14 and only the macOS OS classifier.
Dependency ranges still inherit SQLCyCli; fixed primary CI constraints are not
a full transitive hash lock and do not establish all dependency combinations.

The public repository is https://github.com/AresJef/SQLAero. Build-only run
https://github.com/AresJef/SQLAero/actions/runs/37184075375 passed at b3fd604.
The PyPI pending publisher mapping below is registered. The user explicitly
confirmed removal of the pypi required-reviewer protection: publishing needs
no separate Review deployments approval. No release or PyPI upload has yet
been performed. Pending registration does not reserve the PyPI project name.

## Confirmed publisher mapping

- GitHub owner: AresJef. Repository: SQLAero. Visibility: Public.
- PyPI project: SQLAero. Pending publisher type: GitHub Actions.
- Workflow filename: release.yml (located .github/workflows/release.yml).
- Environment: pypi, with no required-reviewer protection, as authorized.
- Initial tag: v0.1.0a1. GitHub Release must be marked prerelease.

Adding this pending publisher persistently authorizes the exact repository,
workflow and environment to publish the PyPI project. Confirm that mapping
before submitting it. Pending registration does not reserve the name.

## Workflow boundary

Only the published prerelease v0.1.0a1 can reach the publish job, with both
actor and triggering_actor equal to AresJef. Later versions require an explicit
workflow update; this first-alpha workflow is not yet version-generic. Push does not
trigger it. workflow_dispatch builds/tests only and cannot publish. Only the
publish job has id-token: write. All five third-party action references are
pinned to public upstream commit IDs, resolved from official GitHub tag APIs.
No token/password is configured; only the verified wheel artifact is uploaded
to PyPI. sdist remains an intermediate wheel build input, not a PyPI upload.

The CI runner is macos-26 arm64 / Python 3.13 and explicitly builds for macOS
26.0. Build-only run #3 verified the wheel and all 19 native extensions for
arm64 and deployment target 26.0; install and offline checks passed on macOS26.
Existing local test evidence is macOS27.0.1 arm64 / Python3.13.3. No cross-OS
or database compatibility claim is made.

## Existing artifacts

The retained dist/ files are historical local-validation artifacts with the
previous Requires-Python >=3.10 metadata. Do not upload them. The new workflow
must build fresh distributions with the approved metadata and test the actual
new wheel before publication. The historical SHA256SUMS remain unchanged.

## User-operated first release

1. Open https://github.com/AresJef/SQLAero/releases/new as AresJef.
2. Create tag v0.1.0a1 with Target main, mark it Pre-release, and add release notes.
3. Publish release manually. That action starts a fresh build and verification.
4. When all build/install/offline checks pass, PyPI upload runs automatically
   through the pypi Trusted Publisher. No Review deployments step is required.
5. Verify the Actions publish result and the resulting PyPI files/installation.
   Do not create an empty package. Run workflow and ordinary pushes cannot upload.
