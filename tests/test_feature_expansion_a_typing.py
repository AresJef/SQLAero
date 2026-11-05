"""Check the installed final interface with real Pyright, without editor tests."""

import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Any

import sqlaero


def test_installed_configuration_and_helper_types_match_the_final_contract(tmp_path: Path) -> None:
    samples = Path(__file__).with_name("typechecking")
    positive = samples / "feature_config_positive.py"
    negative = samples / "feature_config_negative.py"
    expected = {index for index, line in enumerate(negative.read_text().splitlines()) if "# error:" in line}
    configuration = tmp_path / "pyrightconfig.json"
    configuration.write_text(json.dumps({
        "include": [os.path.relpath(positive, tmp_path), os.path.relpath(negative, tmp_path)],
        "extraPaths": [str(Path(sqlaero.__file__).parent.parent)],
        "typeCheckingMode": "strict", "pythonVersion": "3.12",
    }))
    result = subprocess.run(
        ["pyright", "--outputjson", "--pythonpath", sys.executable, "--project", str(configuration)],
        text=True, capture_output=True, timeout=30, check=False,
    )
    report: dict[str, Any] = json.loads(result.stdout)
    errors = [entry for entry in report["generalDiagnostics"] if entry["severity"] == "error"]
    assert result.returncode == 1 and len(errors) == 27, result.stdout + result.stderr
    assert all(Path(entry["file"]).name == negative.name for entry in errors), result.stdout
    assert {entry["range"]["start"]["line"] for entry in errors} == expected, result.stdout
    assert report["summary"]["warningCount"] == 0, result.stdout
