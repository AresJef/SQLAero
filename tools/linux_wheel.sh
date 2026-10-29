#!/usr/bin/env bash
# One native PyPA container boundary for Linux probes, release and PyPI checks.
# The job owns the server, daemon and private fixture; product code sees none.
set -euo pipefail

side=${1:?host or container}
mode=${2:?probe, build or pypi}
case "$mode" in probe|build|pypi) ;; *) exit 2 ;; esac
case "${LINUX_LIBC:?glibc or musl}" in
  glibc) compatibility=manylinux_2_28; rust_libc=gnu ;;
  musl) compatibility=musllinux_1_2; rust_libc=musl ;;
  *) exit 2 ;;
esac
architecture=$(uname -m)
case "$architecture" in x86_64|aarch64) ;; *) exit 2 ;; esac
case "${PYTHON_MINOR:?normal CPython minor}" in 3.12|3.13|3.14|3.15) ;; *) exit 2 ;; esac
evidence=${EVIDENCE_DIR:-validation-evidence}
wheels=${WHEEL_DIR:-dist}
mkdir -p "$evidence"

if [[ "$side" == host ]]; then
  image="quay.io/pypa/${compatibility}_${architecture}"
  if [[ -f data/ci-linux-images.json ]]; then
    image=$(python -c 'import json,sys; print(json.load(open("data/ci-linux-images.json"))[sys.argv[1]])' \
      "linux-$LINUX_LIBC-$architecture")
  else
    # Initial discovery is allowed only for a development probe. A release
    # cannot silently resolve mutable image tags after its version is frozen.
    [[ "$mode" == probe ]]
  fi
  docker pull "$image"
  export LINUX_IMAGE_DIGEST
  LINUX_IMAGE_DIGEST=$(docker image inspect "$image" --format '{{index .RepoDigests 0}}')
  docker image inspect "$image" --format '{{json .RepoDigests}}' > "$evidence/container-image.json"
  # The runner's packaged CLI requires newer glibc. Use Docker's documented
  # static test distribution without installing a daemon or changing the host.
  # Source: https://docs.docker.com/engine/install/binaries/
  case "$architecture" in
    x86_64) client_sha=4422c550a6583fedfa790e00e7d51c1ee494e5c3f2dffc0a74145417fb9a7a6f ;;
    aarch64) client_sha=916d1a8f6257a26142fe139ed6e4bf18f52d67259e72577b54368202033ee90a ;;
  esac
  client_archive="$SQLAERO_TEST_MYSQL_ROOT/docker-client.tgz"
  curl --fail --location --proto '=https' --proto-redir '=https' --retry 2 --max-time 180 \
    "https://download.docker.com/linux/static/stable/$architecture/docker-29.1.2.tgz" -o "$client_archive"
  printf '%s  %s\n' "$client_sha" "$client_archive" | sha256sum --check
  tar -xzf "$client_archive" -O docker/docker > "$SQLAERO_TEST_MYSQL_ROOT/docker-cli"
  chmod 700 "$SQLAERO_TEST_MYSQL_ROOT/docker-cli"
  sha256sum "$client_archive" "$SQLAERO_TEST_MYSQL_ROOT/docker-cli" > "$evidence/docker-client-sha256.txt"
  docker pull mysql:8.4.13
  docker pull mysql:9.7.4
  EVIDENCE_DIR="$evidence" python - <<'PY'
import json
import os
from pathlib import Path
import shlex
import subprocess
root = Path(os.environ['SQLAERO_TEST_MYSQL_ROOT'])
identities = {}
for label, version in [('84', '8.4.13'), ('97', '9.7.4')]:
    image = subprocess.check_output(
        ['docker', 'image', 'inspect', f'mysql:{version}', '--format', '{{index .RepoDigests 0}}'],
        text=True).strip()
    command = [str(root / 'docker-cli'), 'run', '--rm', '-i', '--network', 'host',
               '--user', f'{os.getuid()}:{os.getgid()}',
               '-v', f'{root}:{root}:ro', '--entrypoint', 'mysql', image]
    wrapper = root / f'mysql-client-{label}'
    wrapper.write_text('#!/bin/sh\nexec ' + shlex.join(command) + ' "$@"\n')
    wrapper.chmod(0o700)
    identities[label] = image
Path(os.environ['EVIDENCE_DIR'], 'mysql-client-images.json').write_text(json.dumps(identities, indent=2))
PY
  python tools/mysql_fixture.py clients \
    --client84 "$SQLAERO_TEST_MYSQL_ROOT/mysql-client-84" \
    --client97 "$SQLAERO_TEST_MYSQL_ROOT/mysql-client-97"
  docker run --rm --network host \
    -v "$GITHUB_WORKSPACE:/work" -w /work \
    -v "$SQLAERO_TEST_MYSQL_ROOT:$SQLAERO_TEST_MYSQL_ROOT" \
    -v /var/run/docker.sock:/var/run/docker.sock \
    -e SQLAERO_TEST_MYSQL_ROOT -e SQLAERO_NATIVE_MYSQL=1 \
    -e SQLAERO_TEST_TLS="$SQLAERO_TEST_MYSQL_ROOT/tls" \
    -e LINUX_LIBC -e PYTHON_MINOR -e LINUX_IMAGE_DIGEST \
    -e EVIDENCE_DIR="$evidence" -e WHEEL_DIR="$wheels" \
    -e EXPECTED_SDIST_SHA -e RELEASE_VERSION \
    -e GIT_CONFIG_COUNT=1 -e GIT_CONFIG_KEY_0=safe.directory -e GIT_CONFIG_VALUE_0=/work \
    "$LINUX_IMAGE_DIGEST" bash tools/linux_wheel.sh container "$mode"
  exit
fi
[[ "$side" == container ]]
"$SQLAERO_TEST_MYSQL_ROOT/docker-cli" version --format '{{.Client.Version}}'
if [[ "$LINUX_LIBC" == musl ]]; then
  apk add --no-cache ca-certificates curl tzdata nodejs patchelf openssl
else
  # Pinned manylinux images already carry these packages. Do not refresh
  # unrelated repositories when the required tools and trust store are present.
  if ! rpm --quiet --query openssl ca-certificates; then
    timeout 180 dnf install -y openssl ca-certificates
  fi
fi
export CARGO_HOME=/tmp/sqlaero-cargo RUSTUP_HOME=/tmp/sqlaero-rustup
abi="cp${PYTHON_MINOR/./}-cp${PYTHON_MINOR/./}"
export PATH="$CARGO_HOME/bin:/opt/python/$abi/bin:$PATH"
curl --proto '=https' --tlsv1.2 --fail --silent --show-error https://sh.rustup.rs -o /tmp/sqlaero-rustup.sh
sh /tmp/sqlaero-rustup.sh -y --profile minimal --default-toolchain 1.99.1 --component clippy,rustfmt --no-modify-path
python -m venv /tmp/sqlaero-original
export PATH="/tmp/sqlaero-original/bin:$PATH" PYO3_PYTHON=/tmp/sqlaero-original/bin/python
export CARGO_TARGET_DIR=/tmp/sqlaero-target-original
target="$architecture-unknown-linux-$rust_libc"
python -m pip install --disable-pip-version-check -r requirements-dev.txt
python -c 'import _testcapi, sys; print(sys.version)'
EVIDENCE_DIR="$evidence" python - <<'PY'
import json
import os
from pathlib import Path
import subprocess
versions = {}
for minor in ['312', '313', '314', '315']:
    executable = f'/opt/python/cp{minor}-cp{minor}/bin/python'
    versions[minor] = subprocess.check_output([executable, '-c', 'import sys; print(sys.version)'], text=True).strip()
Path(os.environ['EVIDENCE_DIR'], 'python-toolchains.json').write_text(json.dumps(versions, indent=2))
PY
rustc -vV > "$evidence/toolchain.txt"

# These few coarse phase records complement Actions step times when container
# work shares one outer step. Failed phases remain failures, never zero times.
phase() {
  local name=$1 started elapsed
  shift
  started=$(date +%s)
  elapsed=$SECONDS
  "$@"
  printf '{"phase":"%s","started_epoch":%s,"ended_epoch":%s,"elapsed_seconds":%s}\n' \
    "$name" "$started" "$(date +%s)" "$((SECONDS - elapsed))" >> "$evidence/phase-times.jsonl"
}
test_package() {
  local interpreter=$1 kind=$2
  "$interpreter" tests/_native_signal.py
  "$interpreter" tests/_native_signal.py --pytest -q tests --junitxml="$evidence/python-$kind.xml"
  "$interpreter" -c 'from pathlib import Path; import sys; from tools.release_artifacts import test_totals; print(test_totals(Path(sys.argv[1])))' \
    "$evidence/python-$kind.xml"
  "$interpreter" -m pip check
}

if [[ "$mode" == pypi ]]; then
  phase pypi-download python tools/release_artifacts.py download \
    --version "$RELEASE_VERSION" --manifest release-manifest.json --destination "$wheels" \
    --output "$evidence/pypi-download.json"
  phase pypi-install python -m pip install --no-deps "$wheels"/*.whl
  python tools/release_artifacts.py identity --kind pypi --wheel "$wheels"/*.whl \
    --output "$evidence/pypi-wheel-identity.json"
  phase pypi-test test_package python pypi
  exit
fi

cargo fmt --all --check
cargo test -p sqlaero-core --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python tools/error_catalog.py --check
pyright --pythonpath "$PYO3_PYTHON"
phase original-build maturin build --release --locked --target "$target" \
  --compatibility "$compatibility" --interpreter python --out "$wheels"
phase original-install python -m pip install --no-deps "$wheels"/*.whl
sdist_arguments=()
if [[ "$mode" == build ]]; then sdist_arguments=(--sdist source-dist/*.tar.gz); fi
python tools/release_artifacts.py identity --kind original --wheel "$wheels"/*.whl \
  "${sdist_arguments[@]}" --output "$evidence/original-wheel-identity.json"
cargo test -p sqlaero-core --lib explicit_roots_keep_platform_ssl_name_usage_and_time_policy --locked -- --ignored --test-threads=1
cargo test -p sqlaero-core --test establish --locked -- --ignored --test-threads=1
cargo test -p sqlaero-core --test component_contracts --locked -- --ignored --test-threads=1
cargo test -p sqlaero-core --test protocol_wire --locked -- --ignored --test-threads=1
if [[ "$PYTHON_MINOR" == 3.13 ]]; then
  cargo test -p sqlaero-core --test system_trust --locked -- --ignored --test-threads=1
fi
phase original-test test_package python original
if [[ "$mode" == probe ]]; then exit; fi
if [[ "$PYTHON_MINOR" == 3.13 ]]; then
  phase diagnostic-smoke python tools/benchmark.py smoke --seconds 180 --output "$evidence"
fi
source_directory=$(python tools/release_artifacts.py extract --sdist source-dist/*.tar.gz \
  --destination sdist-source --expected-sha "$EXPECTED_SDIST_SHA")
rebuild_source() {
  # A distinct target directory forces a real rebuild from the extracted sdist.
  (cd "$source_directory" && CARGO_TARGET_DIR=/tmp/sqlaero-target-sdist \
    "$PYO3_PYTHON" -m maturin build --release --locked --target "$target" \
      --compatibility "$compatibility" --interpreter "$PYO3_PYTHON" --out /work/sdist-wheels)
}
phase sdist-build rebuild_source
python -m venv /tmp/sqlaero-sdist
sdist_python=/tmp/sqlaero-sdist/bin/python
"$sdist_python" -m pip install --disable-pip-version-check -r requirements-dev.txt
phase sdist-install "$sdist_python" -m pip install --no-deps sdist-wheels/*.whl
"$sdist_python" tools/release_artifacts.py identity --kind sdist --wheel sdist-wheels/*.whl \
  --sdist source-dist/*.tar.gz --output "$evidence/sdist-wheel-identity.json"
phase sdist-test test_package "$sdist_python" sdist
