"""Declared unsupported label schemas cannot disappear behind missing codes."""

# pyright: reportPrivateUsage=false


import pytest
from sqlaero import _sqlaero, json_dumps

pd = pytest.importorskip("pandas")


@pytest.mark.parametrize("family", ["period", "interval"])
@pytest.mark.parametrize("kind", ["category", "series", "frame", "multi"])
def test_missing_codes_do_not_hide_unsupported_label_schema(family: str, kind: str) -> None:
    labels = pd.period_range("2026-01-01", periods=2, freq="D") if family == "period" else pd.IntervalIndex.from_breaks([0, 1, 2])
    value = pd.Categorical.from_codes([-1, -1], categories=labels)
    if kind == "series":
        value = pd.Series(value)
    elif kind == "frame":
        value = pd.DataFrame({"value": value})
    elif kind == "multi":
        value = pd.MultiIndex(levels=[pd.CategoricalIndex(value)], codes=[[-1, -1]], verify_integrity=False)
    with pytest.raises(TypeError, match="declared Pandas extension dtype"):
        json_dumps(value)
    with pytest.raises(TypeError, match="declared Pandas extension dtype"):
        _sqlaero._escape_projection(value)
