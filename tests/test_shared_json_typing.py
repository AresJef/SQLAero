"""The new installed JSON exits retain their real public typing contract."""

import json
from pathlib import Path
import subprocess
import sys

import sqlaero


def test_installed_json_exports_and_escape_use_have_precise_types(tmp_path: Path) -> None:
    positive = tmp_path/"json_positive.py"
    positive.write_text('''
from typing import assert_type
from enum import Enum, Flag
from uuid import UUID
from sqlaero import Connection, JSON, json_dumps
from sqlaero.typing import EscapeInput

wrapped = JSON({"id": 7}, allow_nan=False)
assert_type(wrapped, JSON)
assert_type(wrapped.data, object)
assert_type(wrapped.allow_nan, bool)
assert_type(json_dumps({"id": 7}), bytes)
assert_type(json_dumps(obj=[7], allow_nan=False), bytes)

def use(connection: Connection) -> None:
    value: EscapeInput = JSON({"id": 7})
    connection.escape(value)
    cursor = connection.cursor()
    cursor.execute("SELECT %s", (value,))
    assert_type(cursor.mogrify("SELECT %s", (value,)), str | list[str])

class Number(Enum):
    ACTUAL = 7
class Permission(Flag):
    READ = 1
    WRITE = 2
def accepted_scalars(connection: Connection) -> None:
    for source in (JSON({"id": 7}), UUID(int=7), Number.ACTUAL, Permission.READ):
        value: EscapeInput = source
        connection.cursor().execute("SELECT %s", value)
''')
    negative = tmp_path/"json_negative.py"
    negative.write_text('''
from sqlaero import JSON, json_dumps
from sqlaero.typing import EscapeInput
wrapped = JSON({})
JSON()  # error:
JSON({}, False)  # error:
JSON({}, allow_nan=1)  # error:
JSON(data={})  # error:
wrapped.data = {}  # error:
wrapped.allow_nan = False  # error:
json_dumps()  # error:
json_dumps({}, False)  # error:
json_dumps({}, allow_nan="yes")  # error:
class Child(JSON):  # error:
    pass
unsupported: EscapeInput = object()  # error:
''')
    expected = {i for i,line in enumerate(negative.read_text().splitlines()) if "# error:" in line}
    config = tmp_path/"pyrightconfig.json"
    config.write_text(json.dumps({
        "include":[str(positive),str(negative)],
        "extraPaths":[str(Path(sqlaero.__file__).parent.parent)],
        "typeCheckingMode":"strict","pythonVersion":"3.12",
    }))
    result = subprocess.run(
        ["pyright","--outputjson","--pythonpath",sys.executable,"--project",str(config)],
        capture_output=True,text=True,timeout=30,check=False,
    )
    report = json.loads(result.stdout)
    errors = [x for x in report["generalDiagnostics"] if x["severity"]=="error"]
    assert result.returncode==1 and errors,result.stdout+result.stderr
    assert all(Path(x["file"])==negative for x in errors),result.stdout
    assert {x["range"]["start"]["line"] for x in errors}==expected,result.stdout
    assert report["summary"]["warningCount"]==0,result.stdout
