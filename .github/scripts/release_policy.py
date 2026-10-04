"""Version and event policy for owner-operated SQLAero releases."""
import configparser
import json
import os
from pathlib import Path
from packaging.specifiers import SpecifierSet
from packaging.version import Version


def read_package(path=Path("setup.cfg")):
    cfg = configparser.ConfigParser()
    assert cfg.read(path), "Missing package metadata"
    assert cfg["metadata"]["name"] == "SQLAero", "Unexpected package name"
    raw = cfg["metadata"]["version"]
    version = Version(raw)
    assert raw == str(version), "Use a canonical PEP 440 version"
    assert version.local is None, "PyPI does not accept local versions"
    assert SpecifierSet(cfg["options"]["python_requires"]) == SpecifierSet(">=3.13,<3.14")
    return version


def release_allowed(version, event_name, actor, triggering_actor, repository, event):
    """Return false for non-release/non-owner events; reject malformed releases."""
    if (event_name != "release" or event.get("action") != "published"
            or actor != "AresJef" or triggering_actor != "AresJef"
            or repository != "AresJef/SQLAero"):
        return False
    release = event.get("release", {})
    tag = release.get("tag_name", "")
    assert tag.startswith("v"), "Release tag must start with v"
    tag_version = Version(tag[1:])
    assert tag == "v" + str(tag_version), "Use a canonical v+PEP440 tag"
    assert tag_version == version, "Release tag and package version differ"
    assert release.get("prerelease") is version.is_prerelease, "Pre-release label differs from version"
    assert release.get("draft") is False, "Release is still a draft"
    return True


if __name__ == "__main__":
    version = read_package()
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    allowed = release_allowed(version, os.environ["GITHUB_EVENT_NAME"],
        os.environ["GITHUB_ACTOR"], os.environ["GITHUB_TRIGGERING_ACTOR"],
        os.environ["GITHUB_REPOSITORY"], event)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"version={version}\n")
        output.write(f"wheel_filename=sqlaero-{version}-cp313-cp313-macosx_26_0_arm64.whl\n")
        output.write(f"publish_allowed={str(allowed).lower()}\n")
    print(f"Package version: {version}; publish allowed: {allowed}")
