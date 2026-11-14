"""Target schema admission is independent of missing category/level codes."""

import pytest
from sqlaero import _sqlaero, json_dumps

pd = pytest.importorskip("pandas")
np = pytest.importorskip("numpy")
pa = pytest.importorskip("pyarrow")


@pytest.mark.parametrize("family", ["complex", "arrow_binary", "arrow_unknown"])
@pytest.mark.parametrize("kind", ["category", "multi"])
@pytest.mark.parametrize("empty", [False, True])
def test_unused_declared_schema_is_admitted_without_acquiring_label_values(family, kind, empty, monkeypatch, record_property) -> None:
    if family == "complex":
        labels = pd.Index(np.array([] if empty else [1 + 2j], dtype="complex128"))
    else:
        dtype = pa.binary() if family == "arrow_binary" else pa.float16()
        labels = pd.Index(pd.array([] if empty else [b"a"] if family == "arrow_binary" else [1.0], dtype=pd.ArrowDtype(dtype)))
    try:
        value = pd.Categorical.from_codes([-1, -1], categories=labels) if kind == "category" else pd.MultiIndex(levels=[labels], codes=[[-1, -1]], verify_integrity=False)
    except TypeError as error:
        # Pandas3.1.0's float16 Index uniqueness engine rejects this actual
        # source before SQLAero runs. If a provider version permits it, the
        # ordinary product assertions below still apply.
        if family != "arrow_unknown" or kind != "category" or "expected numpy.ndarray, got ArrowExtensionArray" not in str(error):
            raise
        record_property("verification_boundary", "provider-construction-only")
        record_property("provider_error", str(error))
        record_property("provider_versions", f"pandas={pd.__version__};pyarrow={pa.__version__}")
        return
    def forbidden(self):
        raise AssertionError("unreferenced label values were acquired")
    monkeypatch.setattr(pd.Index, "array", property(forbidden))
    with pytest.raises(TypeError, match="dtype|schema|[Bb]inary"):
        json_dumps(value)
    if family == "arrow_binary":
        assert _sqlaero._escape_projection(value, many=True) == (["NULL", "NULL"] if kind == "category" else [("NULL",), ("NULL",)])
    else:
        with pytest.raises(TypeError, match="dtype|schema"):
            _sqlaero._escape_projection(value)


def test_generic_object_labels_are_not_eagerly_examined_when_unused(monkeypatch) -> None:
    labels = pd.Index([object()], dtype=object)
    value = pd.Categorical.from_codes([-1, -1], categories=labels)
    def forbidden(self):
        raise AssertionError("unreferenced Object labels were acquired")
    monkeypatch.setattr(pd.Index, "array", property(forbidden))
    assert json_dumps(value) == b"[null,null]"


@pytest.mark.parametrize("dtype", ["string[python]", "string[pyarrow]", "Int64", "datetime64[ns]"])
def test_supported_declared_labels_do_not_need_their_value_array_when_unused(dtype, monkeypatch) -> None:
    labels = pd.Index(pd.array(["a"] if dtype.startswith("string") else [7], dtype=dtype))
    value = pd.Categorical.from_codes([-1, -1], categories=labels)
    def forbidden(self):
        raise AssertionError("unreferenced typed labels were acquired")
    monkeypatch.setattr(pd.Index, "array", property(forbidden))
    assert json_dumps(value) == b"[null,null]"
