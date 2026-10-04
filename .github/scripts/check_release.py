"""Check the single approved alpha wheel before its CI artifact is published."""
from email.parser import BytesParser
from pathlib import Path
from packaging.specifiers import SpecifierSet
import hashlib
import sys
import zipfile


def check_release(dist: Path) -> None:
    """Verify namespace, metadata, extension count, declarations and original notices.

    Args:
        dist: Directory containing the newly built wheel and source archive.

    Raises:
        AssertionError: A file or metadata field differs from the approved alpha.
    """
    wheels = list(dist.glob("*.whl"))
    assert len(wheels) == 1
    wheel = wheels[0]
    assert wheel.name == "sqlaero-0.1.0a1-cp313-cp313-macosx_26_0_arm64.whl"
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        metadata = BytesParser().parsebytes(archive.read(next(n for n in names if n.endswith("/METADATA"))))
        assert metadata["Name"] == "SQLAero"
        assert metadata["Version"] == "0.1.0a1"
        assert SpecifierSet(metadata["Requires-Python"]) == SpecifierSet(">=3.13,<3.14")
        assert not any(n.startswith("sqlcycli/") for n in names)
        assert len([n for n in names if n.endswith(".so")]) == 19
        for suffix in ["transcode.pxd", "utils.pxd", "connection.pyi"]:
            assert "sqlaero/" + suffix in names
        license_file = next(n for n in names if n.endswith("/LICENSE"))
        assert archive.read(license_file) == Path("LICENSE").read_bytes()
    for artifact in sorted(dist.iterdir()):
        if artifact.is_file():
            print(hashlib.sha256(artifact.read_bytes()).hexdigest(), artifact.name)


if __name__ == "__main__":
    check_release(Path(sys.argv[1]))
