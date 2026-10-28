"""A measurement boundary must execute the existing production value visitor."""

# pyright: reportPrivateUsage=false

from datetime import date, datetime, time, timedelta
from decimal import Decimal
from argparse import Namespace
from pathlib import Path

import pytest

from sqlaero import _sqlaero
from tools.benchmark import collect, quality


def test_mixed_valid_and_misspelled_case_ids_do_not_silently_reduce_coverage(tmp_path: Path) -> None:
    package = str(Path(_sqlaero.__file__).parent.parent)
    arguments = Namespace(output=str(tmp_path / 'run'), legacy=False, prefix=[],
                          case=['escape/none/v0/scalar', 'execute/84/unix/select/R100'],
                          reference=package, candidate=package, config=None,
                          command='calibrate', cycles=1, seconds=1,
                          target_ms=2, minimum_ms=1)
    with pytest.raises(ValueError, match='Unknown or inapplicable case IDs'):
        collect(arguments)
    assert not (tmp_path / 'run').exists()


@pytest.mark.parametrize("value", [
    None, True, 123, -0.0, Decimal("1.2300"), "a'\\\x00中文", b"\x00'\\",
    bytearray(b"abc"), date(2024, 2, 29), datetime(2024, 2, 29, 3, 4, 5),
    time(3, 4, 5, 123), timedelta(days=-4, microseconds=5),
    [], (), [1, "x", None], (True, [2, ("中",)]),
])
@pytest.mark.parametrize("no_backslash", [False, True])
def test_native_owner_matches_production_binding(value: object, no_backslash: bool) -> None:
    owner = _sqlaero._encode_value(value, no_backslash=no_backslash)
    request = _sqlaero._bind_sql("SELECT %s", value, itemize=False, no_backslash=no_backslash)[0]
    assert owner.to_bytes() == request[len(b"\x03SELECT "):]
    assert _sqlaero._prepare_encoder(no_backslash=no_backslash).encode(value).to_bytes() == owner.to_bytes()
    assert len(owner) == len(owner.to_bytes())


def test_native_owner_retains_completed_bytes_independently() -> None:
    value = [1, 2]
    owner = _sqlaero._encode_value(value)
    value.clear()
    assert owner.to_bytes() == b"(1,2)"
    with pytest.raises(TypeError):
        _sqlaero._encode_value([[[object()]]])


def test_cpu_snapshot_allowance_is_not_wall_timer_precision() -> None:
    sample = {"wall_ns": 2_000_000, "cpu_ns": 2_000_000, "cpu_quality": True,
              "snapshot_ns": 1000,
              "clock": {"margin_ns": 30_000, "timer_read_allowance_ns": 200}}
    assert quality(sample, 1_000_000) is None
    sample["cpu_ns"] = 1_500_000
    assert quality(sample, 1_000_000) == "off_cpu_proxy"
