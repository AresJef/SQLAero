# SQLAero release process

The user confirmed CPython 3.13 / macOS 26.0 arm64 wheel-only publication.
setup.cfg now declares Python >=3.13,<3.14 and only the macOS OS classifier.
Dependency ranges still inherit SQLCyCli; fixed primary CI constraints are not
a full transitive hash lock and do not establish all dependency combinations.

The public repository is https://github.com/AresJef/SQLAero. Build-only run
https://github.com/AresJef/SQLAero/actions/runs/37184075375 passed at b3fd604.
The PyPI pending publisher mapping below is registered. The user explicitly
confirmed removal of the pypi required-reviewer protection: publishing needs
no separate Review deployments approval. The first release and PyPI upload completed successfully; see RELEASE_VALIDATION.md.
The original pending registration did not reserve the PyPI project name.

## Confirmed publisher mapping

- GitHub owner: AresJef. Repository: SQLAero. Visibility: Public.
- PyPI project: SQLAero. Pending publisher type: GitHub Actions.
- Workflow filename: release.yml (located .github/workflows/release.yml).
- Environment: pypi, with no required-reviewer protection, as authorized.
- Release tag: v followed by the canonical PEP 440 version in setup.cfg.
- GitHub Pre-release must match whether that version is a prerelease.

Adding this pending publisher persistently authorizes the exact repository,
workflow and environment to publish the PyPI project. Confirm that mapping
before submitting it. Pending registration does not reserve the name.

## Workflow boundary

The reusable workflow validates tag/version and the GitHub Pre-release label
against setup.cfg. Only a published Release, with both actor and triggering_actor
equal to AresJef, can reach the publish job. Push does not
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

## Next releases

The reusable workflow is prepared locally for review and is not pushed yet.
After that workflow change is reviewed and pushed, each release requires:

1. Change only the package version in setup.cfg to a new, canonical PEP 440
   version, such as 0.1.0a2, 0.1.0rc1 or 0.1.0. Do not reuse uploaded versions.
2. Review intended source/dependency changes and release notes; run applicable
   tests, commit and push. Run workflow can verify the new version without upload.
3. As AresJef, manually create a GitHub Release with tag v plus that exact version,
   targeting the reviewed commit. Select Pre-release for alpha/beta/rc/dev versions;
   leave it off for stable/post releases. Publish release manually.
4. The tagged source is freshly built and verified. Only the validated wheel is
   uploaded automatically through OIDC after all checks pass; no separate
   Review deployments approval or persistent API token is required.
5. Check the Actions publish result, PyPI files/hash and fresh PyPI installation.
   If an uploaded release has a defect, plan a new version; never move the old tag
   or try to overwrite a published file.

The workflow never creates tags/Releases. Push, PR, dispatch, non-owner actions
and non-owner reruns cannot upload. The platform, ABI, wheel-only policy, native
checks and dependency/test scope remain fixed. Version parsing alone does not
make a release production-ready; database/compatibility validation is separate.
