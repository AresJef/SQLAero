# First alpha publishing preparation

The user confirmed CPython 3.13 / macOS arm64 wheel-only publication.
setup.cfg now declares Python >=3.13,<3.14 and only the macOS OS classifier.
Dependency ranges still inherit SQLCyCli; fixed primary CI constraints are not
a full transitive hash lock and do not establish all dependency combinations.

As of this preparation, Chrome is logged in as AresJef. The logged-in target
https://github.com/AresJef/SQLAero shows 404. No repository has been created.
Local Git was initialized for the user-operated VS Code Publish flow.
No remote, push, release or publisher registration was performed.

## Mapping requiring actual authorization at configuration time

- GitHub owner: AresJef. Repository: SQLAero. Visibility: Public.
- PyPI project: SQLAero. Pending publisher type: GitHub Actions.
- Workflow filename: release.yml (located .github/workflows/release.yml).
- Environment: pypi, preferably protected by a release reviewer.
- Initial tag: v0.1.0a1. GitHub Release must be marked prerelease.

Adding this pending publisher persistently authorizes the exact repository,
workflow and environment to publish the PyPI project. Confirm that mapping
before submitting it. Pending registration does not reserve the name.

## Workflow boundary

Only published releases can reach the publish job. First push does not
trigger it. workflow_dispatch builds/tests only and cannot publish. Only the
publish job has id-token: write. All five third-party action references are
pinned to public upstream commit IDs, resolved from official GitHub tag APIs.
No token/password is configured; only the verified wheel artifact is uploaded
to PyPI. sdist remains an intermediate wheel build input, not a PyPI upload.

The CI runner is macos-26 arm64 / Python 3.13 and explicitly builds for macOS
26.0. Until that CI succeeds, macOS26 is an intended deployment target only.
Existing local test evidence is macOS27.0.1 arm64 / Python3.13.3. No cross-OS
or database compatibility claim is made.

## Existing artifacts

The retained dist/ files are historical local-validation artifacts with the
previous Requires-Python >=3.10 metadata. Do not upload them. The new workflow
must build fresh distributions with the approved metadata and test the actual
new wheel before publication. The historical SHA256SUMS remain unchanged.

## Next joint steps

1. The user will publish through VS Code after reviewing the local commit.
2. Create AresJef/SQLAero publicly and push reviewed source/workflow.
3. Run workflow_dispatch; inspect successful build/install/offline-test output.
4. Configure pypi environment and confirm/register the exact pending publisher.
5. Jointly publish prerelease v0.1.0a1, approve the publishing job and verify
   the resulting PyPI files/installation. Do not create an empty package.
