"""Check the installed final interface with real Pyright, without editor tests."""

import json
from pathlib import Path
import subprocess
import sys
from typing import Any

import sqlaero
import pytest


@pytest.mark.parametrize(("stem", "diagnostic_count"), [("feature_config", 27), ("feature_cursor", 42)])
def test_installed_final_types_match_the_contract(
    tmp_path: Path, stem: str, diagnostic_count: int,
) -> None:
    samples = Path(__file__).with_name("typechecking")
    positive = samples / f"{stem}_positive.py"
    # Negative fixtures are text so the repository-wide strict Pyright gate
    # does not treat deliberately invalid consumers as production source.
    negative = tmp_path / f"{stem}_negative.py"
    negative.write_text((samples / f"{stem}_negative.py.txt").read_text())
    expected = {index for index, line in enumerate(negative.read_text().splitlines()) if "# error:" in line}
    configuration = tmp_path / "pyrightconfig.json"
    configuration.write_text(json.dumps({
        "include": [str(positive.resolve()), str(negative.resolve())],
        "extraPaths": [str(Path(sqlaero.__file__).parent.parent)],
        "typeCheckingMode": "strict", "pythonVersion": "3.12",
    }))
    result = subprocess.run(
        ["pyright", "--outputjson", "--pythonpath", sys.executable, "--project", str(configuration)],
        text=True, capture_output=True, timeout=30, check=False,
    )
    report: dict[str, Any] = json.loads(result.stdout)
    errors = [entry for entry in report["generalDiagnostics"] if entry["severity"] == "error"]
    assert result.returncode == 1 and len(errors) == diagnostic_count, result.stdout + result.stderr
    assert all(Path(entry["file"]).name == negative.name for entry in errors), result.stdout
    assert {entry["range"]["start"]["line"] for entry in errors} == expected, result.stdout
    assert report["summary"]["warningCount"] == 0, result.stdout
