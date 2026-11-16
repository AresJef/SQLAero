"""Isolate the exact provider construction from SQLAero's masked-value path.

This temporary CI diagnostic does not import SQLAero. A source-level native
failure is saved with its last completed operation instead of killing pytest.
"""

from __future__ import annotations

from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys


SCRIPT = """
import sys
import numpy as np
mode, unit, structured = sys.argv[1], sys.argv[2], sys.argv[3] == '1'
assert 'sqlaero' not in sys.modules
print('provider', np.__version__, 'mode', mode, 'unit', unit, 'structured', structured, flush=True)
raw = np.array([1], dtype='i8').view(np.dtype('M8' if unit == 'generic' else 'M8[' + unit + ']'))
print('raw_ready', flush=True)
if structured:
    data = np.empty(1, dtype=[('at', raw.dtype)])
    print('data_allocated', flush=True)
    if mode == 'old':
        data['at'] = raw
    else:
        data['at'].view('i8')[:] = raw.view('i8')
    print('payload_assigned', flush=True)
else:
    data = raw
if structured and unit in ('generic', '0ns'):
    value = np.ma.array(np.zeros(1, dtype=[('at', 'M8[ns]')]), mask=True)
    print('mask_base_ready', flush=True)
    value.dtype = data.dtype
    print('mask_dtype_ready', flush=True)
    value.data['at'].view('i8')[:] = 1
else:
    value = np.ma.array(data, mask=True)
print('mask_ready', flush=True)
ticks = value.data['at'].view('i8') if structured else value.data.view('i8')
mask = value.mask['at'] if structured else value.mask
assert ticks.tolist() == [1] and mask.tolist() == [True]
assert value.shape == (1,)
print('source_verified_without_sqlaero', flush=True)
"""


def main() -> None:
    records: list[dict[str, object]] = []
    for mode in ("old", "bits"):
        for structured in (False, True):
            for unit in ("ps", "generic", "0ns"):
                result = subprocess.run(
                    [sys.executable, "-X", "faulthandler", "-u", "-c", SCRIPT,
                     mode, unit, "1" if structured else "0"],
                    capture_output=True, text=True, timeout=30, check=False,
                )
                records.append({"mode": mode, "structured": structured, "unit": unit,
                                "returncode": result.returncode,
                                "stdout": result.stdout, "stderr": result.stderr})
                print(json.dumps(records[-1]), flush=True)
    output = Path("validation-evidence/numpy-temporal-source-probe.json")
    output.parent.mkdir(exist_ok=True)
    output.write_text(json.dumps({"at": datetime.now(timezone.utc).isoformat(),
                                  "python": sys.version, "records": records}, indent=2) + "\n")
    assert all(row["returncode"] == 0 for row in records if row["mode"] == "bits"), records


if __name__ == "__main__":
    main()
