"""Check the single approved alpha wheel before its CI artifact is published."""
from email.parser import BytesParser
from pathlib import Path
from packaging.specifiers import SpecifierSet
from packaging.tags import parse_tag
from packaging.utils import parse_wheel_filename
from packaging.version import Version
import os
import platform
import sysconfig
import subprocess
import tempfile
import re
import hashlib
import sys
import zipfile


def validate_tags(tags) -> None:
    """Keep the first alpha limited to CPython 3.13, macOS 26.0, arm64."""
    assert tags, "Wheel has no compatibility tags"
    for tag in tags:
        assert tag.interpreter == "cp313", f"Unexpected interpreter: {tag}"
        assert tag.abi == "cp313", f"Unexpected ABI: {tag}"
        assert tag.platform == "macosx_26_0_arm64", f"Unexpected platform: {tag}"


def validate_binary(path: Path) -> None:
    """Check the actual Mach-O architecture and deployment target, not just tags."""
    archs = subprocess.check_output(["/usr/bin/lipo", "-archs", str(path)], text=True).split()
    assert archs == ["arm64"], f"Unexpected native architectures: {archs}"
    load_commands = subprocess.check_output(["/usr/bin/otool", "-l", str(path)], text=True)
    blocks = re.findall(r"cmd LC_(?:BUILD_VERSION|VERSION_MIN_MACOSX)\n.*?(?=Load command|\Z)", load_commands, re.S)
    targets = []
    for block in blocks:
        if block.startswith("cmd LC_BUILD_VERSION"):
            assert re.search(r"^\s*platform (?:1|macos)\s*$", block, re.M), "Unexpected Mach-O OS"
        field = "minos" if block.startswith("cmd LC_BUILD_VERSION") else "version"
        targets.extend(re.findall(r"^\s*" + field + r" (\d+\.\d+(?:\.\d+)?)\s*$", block, re.M))
    assert targets, "Missing Mach-O deployment target"
    assert all(Version(t) == Version("26.0") for t in targets), f"Unexpected deployment targets: {targets}"


def check_release(dist: Path) -> None:
    """Verify namespace, metadata, extension count, declarations and original notices.

    Args:
        dist: Directory containing the newly built wheel and source archive.

    Raises:
        AssertionError: A file or metadata field differs from the approved alpha.
    """
    wheels = list(dist.glob("*.whl"))
    print("Produced wheels:", [w.name for w in wheels], flush=True)
    print("Build platform:", sysconfig.get_platform(), "host:", platform.mac_ver()[0],
          "deployment target:", os.environ.get("MACOSX_DEPLOYMENT_TARGET"), flush=True)
    assert len(wheels) == 1, "Expected exactly one wheel"
    wheel = wheels[0]
    name, version, build, filename_tags = parse_wheel_filename(wheel.name)
    print("Filename tags:", sorted(map(str, filename_tags)), flush=True)
    assert name == "sqlaero", f"Unexpected distribution: {name}"
    assert version == Version("0.1.0a1"), f"Unexpected version: {version}"
    assert not build, f"Unexpected build tag: {build}"
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        wheel_files = [n for n in names if n.endswith(".dist-info/WHEEL")]
        assert len(wheel_files) == 1, "Expected exactly one WHEEL metadata file"
        wheel_metadata = BytesParser().parsebytes(archive.read(wheel_files[0]))
        internal_tags = set()
        for value in wheel_metadata.get_all("Tag", []):
            internal_tags.update(parse_tag(value))
        print("Internal WHEEL tags:", sorted(map(str, internal_tags)), flush=True)
        assert wheel_metadata["Wheel-Version"] == "1.0"
        assert wheel_metadata["Root-Is-Purelib"] == "false"
        assert internal_tags == filename_tags, "Filename and WHEEL tags differ"
        validate_tags(filename_tags)
        metadata = BytesParser().parsebytes(archive.read(next(n for n in names if n.endswith("/METADATA"))))
        assert metadata["Name"] == "SQLAero"
        assert metadata["Version"] == "0.1.0a1"
        assert SpecifierSet(metadata["Requires-Python"]) == SpecifierSet(">=3.13,<3.14")
        assert not any(n.startswith("sqlcycli/") for n in names)
        extensions = [n for n in names if n.endswith(".so")]
        assert len(extensions) == 19
        with tempfile.TemporaryDirectory() as temporary:
            for index, name in enumerate(extensions):
                path = Path(temporary) / f"extension-{index}.so"
                path.write_bytes(archive.read(name))
                validate_binary(path)
        print("Verified 19 native extensions: arm64, deployment target 26.0", flush=True)
        for suffix in ["transcode.pxd", "utils.pxd", "connection.pyi"]:
            assert "sqlaero/" + suffix in names
        license_file = next(n for n in names if n.endswith("/LICENSE"))
        assert archive.read(license_file) == Path("LICENSE").read_bytes()
    for artifact in sorted(dist.iterdir()):
        if artifact.is_file():
            print(hashlib.sha256(artifact.read_bytes()).hexdigest(), artifact.name)


if __name__ == "__main__":
    check_release(Path(sys.argv[1]))
