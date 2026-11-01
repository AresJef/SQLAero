"""Installed Escape inputs through the public Cursor and owned MySQL servers."""
# pyright: reportPrivateUsage=false

from datetime import datetime, timedelta
from decimal import Decimal, ROUND_HALF_UP, localcontext
import pytest

from test_public_connection import live, requires_mysql


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("use_decimal", [False, True])
def test_high_scale_decimal_literal_round_trip_and_target_scale(label: str, use_decimal: bool) -> None:
    with live(label) as connection, connection.cursor() as cursor:
        cursor.use_decimal = use_decimal
        for source in [Decimal("1e-31"), Decimal("-1e-65"), Decimal("0." + "12345" * 13)]:
            cursor.execute("SELECT %s", (source,))
            actual = cursor.fetchone()
            assert actual is not None
            expected = source if use_decimal else float(source)
            assert actual == (expected,)
            assert isinstance(actual[0], Decimal if use_decimal else float)
            exponent = source.as_tuple().exponent
            assert isinstance(exponent, int)
            assert cursor.columns[0].scale == -exponent
        cursor.execute("CREATE TEMPORARY TABLE decimal_input_probe (value DECIMAL(65,30))")
        values = [Decimal("1e-31"), Decimal("-1e-65"), Decimal("0.12345678901234567890123456789015"), Decimal("-0.12345678901234567890123456789015")]
        cursor.execute("INSERT INTO decimal_input_probe VALUES (%s)", [(value,) for value in values], many=True)
        assert cursor.row_count == len(values)
        cursor.execute("SELECT value FROM decimal_input_probe")
        with localcontext() as context:
            context.prec = 80
            expected = [value.quantize(Decimal("1e-30"), rounding=ROUND_HALF_UP) for value in values]
        if not use_decimal:
            expected = [float(value) for value in expected]
        assert cursor.fetchall() == tuple((value,) for value in expected)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("truncate", [False, True])
def test_source_nanoseconds_reach_mysql_for_datetime_and_time(label: str, truncate: bool) -> None:
    np = pytest.importorskip("numpy")
    with live(label) as connection, connection.cursor() as cursor:
        mode = "STRICT_ALL_TABLES" + (",TIME_TRUNCATE_FRACTIONAL" if truncate else "")
        cursor.execute("SET SESSION sql_mode=%s", (mode,))
        stamp = np.datetime64("2024-01-02T03:04:05.123456789", "ns")
        duration = np.timedelta64(123456789, "ns")
        cursor.execute("SELECT CAST(%s AS DATETIME(6)),CAST(%s AS TIME(6))", (stamp, duration))
        microsecond = 123456 if truncate else 123457
        assert cursor.fetchone() == (datetime(2024, 1, 2, 3, 4, 5, microsecond), timedelta(microseconds=microsecond))
        cursor.execute("CREATE TEMPORARY TABLE temporal_input_probe (d DATETIME(6), t TIME(6))")
        cursor.execute("INSERT INTO temporal_input_probe VALUES (%s,%s)", (stamp, duration))
        cursor.execute("SELECT d,t FROM temporal_input_probe")
        assert cursor.fetchone() == (datetime(2024, 1, 2, 3, 4, 5, microsecond), timedelta(microseconds=microsecond))


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("provider", ["numpy", "pandas", "arrow", "polars"])
def test_natural_table_source_uses_public_cursor_batch_and_actual_rows(label: str, provider: str) -> None:
    if provider == "numpy":
        np = pytest.importorskip("numpy")
        source = np.array([[1, 2], [3, 4]], dtype=np.int64)
    elif provider == "pandas":
        pd = pytest.importorskip("pandas")
        source = pd.DataFrame({"x": pd.array([1, 3], dtype="Int64"), "y": pd.array([2, 4], dtype="Int64")})
    elif provider == "arrow":
        pa = pytest.importorskip("pyarrow")
        source = pa.table({"x": [1, 3], "y": [2, 4]})
    else:
        pl = pytest.importorskip("polars")
        source = pl.DataFrame({"x": [1, 3], "y": [2, 4]})
    with live(label) as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE typed_table_probe (x BIGINT, y BIGINT)")
        cursor.execute("INSERT INTO typed_table_probe VALUES (%s,%s)", source)
        assert cursor.row_count == 2
        cursor.execute("SELECT x,y FROM typed_table_probe ORDER BY x")
        assert cursor.fetchall() == ((1, 2), (3, 4))


def table_input(provider: str, rows: list[tuple[int, object]], *, objects: bool = False) -> object:
    """Construct the same logical rows using each provider's actual storage."""
    if provider == "numpy":
        np = pytest.importorskip("numpy")
        return np.array(rows, dtype=object)
    if provider == "pandas":
        pd = pytest.importorskip("pandas")
        return pd.DataFrame({"id": [row[0] for row in rows],
                             "value": pd.Series([row[1] for row in rows], dtype=object)})
    if provider.startswith("pandas-category-"):
        pd = pytest.importorskip("pandas")
        storage = provider.removeprefix("pandas-category-")
        if storage == "pyarrow":
            pytest.importorskip("pyarrow")
        values = pd.array([row[1] for row in rows], dtype=pd.StringDtype(storage=storage))
        return pd.DataFrame({"id": [row[0] for row in rows], "value": pd.Categorical(values)})
    if provider == "arrow":
        pa = pytest.importorskip("pyarrow")
        return pa.table({"id": [row[0] for row in rows], "value": [row[1] for row in rows]})
    pl = pytest.importorskip("polars")
    values = [row[1] for row in rows]
    return pl.DataFrame({"id": [row[0] for row in rows],
                         "value": pl.Series("value", values, dtype=pl.Object) if objects else values})


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("provider", ["numpy", "pandas", "arrow", "polars", "pandas-category-python", "pandas-category-pyarrow"])
def test_typed_table_rollover_preserves_every_actual_row(label: str, provider: str) -> None:
    count = 600 if provider == "pandas-category-python" else 100
    rows: list[tuple[int, object]] = [(index, "x'\\" * 30) for index in range(count)]
    source = table_input(provider, rows)
    with live(label, max_allowed_packet="2KB") as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE typed_rollover_probe (id INT PRIMARY KEY,value VARCHAR(128))")
        assert cursor.execute("INSERT INTO typed_rollover_probe VALUES (%s,%s)", source) == count
        assert cursor.row_count == count and cursor.warning_count == 0
        cursor.execute("SELECT id,value FROM typed_rollover_probe ORDER BY id")
        assert cursor.fetchall() == tuple(rows)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("provider", ["numpy", "pandas", "polars"])
def test_typed_later_callback_failure_keeps_original_and_confirmed_prefix(label: str, provider: str) -> None:
    original = MemoryError("actual later typed-cell conversion failure")
    class Broken(int):
        def __int__(self) -> int:
            raise original
    rows: list[tuple[int, object]] = [(index, "x" * 100) for index in range(100)]
    rows[70] = (70, Broken(7))
    source = table_input(provider, rows, objects=True)
    with live(label, max_allowed_packet="2KB") as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE typed_failure_probe (id INT PRIMARY KEY,value VARCHAR(128))")
        with pytest.raises(MemoryError) as caught:
            cursor.execute("INSERT INTO typed_failure_probe VALUES (%s,%s)", source)
        assert caught.value is original
        assert cursor.row_count is None and cursor.rowcount == -1 and cursor.description is None
        notes = "\n".join(getattr(original, "__notes__", ()))
        assert "[70]" in notes and "confirmed " in notes and "not sent" in notes
        cursor.execute("SELECT COUNT(*) FROM typed_failure_probe")
        count = cursor.fetchone()
        assert count is not None and isinstance(count[0], int) and 0 < count[0] < 70
        cursor.execute("SELECT 42")
        assert cursor.fetchone() == (42,)


@requires_mysql
@pytest.mark.parametrize("label", ["84", "97"])
@pytest.mark.parametrize("provider", ["numpy", "pandas", "arrow", "polars", "pandas-category-python", "pandas-category-pyarrow"])
def test_typed_rows_use_confirmed_changed_quoting_context(label: str, provider: str) -> None:
    values = ["first'\\value", "next'\\value", "last'\\value"]
    if provider == "numpy":
        np = pytest.importorskip("numpy")
        source = np.array(values, dtype=object)
    elif provider == "pandas":
        pd = pytest.importorskip("pandas")
        source = pd.Series(values)
    elif provider.startswith("pandas-category-"):
        pd = pytest.importorskip("pandas")
        storage = provider.removeprefix("pandas-category-")
        if storage == "pyarrow":
            pytest.importorskip("pyarrow")
        labels = pd.Index(pd.array(values, dtype=pd.StringDtype(storage=storage)))
        count = 600 if storage == "python" else 40
        values = (values * ((count + 2) // 3))[:count]
        source = pd.Categorical.from_codes([index % 3 for index in range(count)], categories=labels)
    elif provider == "arrow":
        pa = pytest.importorskip("pyarrow")
        labels = values
        values = (labels * 14)[:40]
        # Keep one long dictionary across the first confirmed mode change;
        # a chunk rollover would clear the cache independently of Context.
        source = pa.DictionaryArray.from_arrays(
            pa.array([index % 3 for index in range(40)], type=pa.int8()),
            pa.array(labels),
        )
    else:
        pl = pytest.importorskip("polars")
        source = pl.concat([pl.Series(values[:1]), pl.Series(values[1:])], rechunk=False)
    with live(label) as connection, connection.cursor() as cursor:
        cursor.execute("CREATE TEMPORARY TABLE typed_context_probe (id INT AUTO_INCREMENT PRIMARY KEY,value VARCHAR(128))")
        cursor.execute("DROP PROCEDURE IF EXISTS sqlaero_typed_change_context")
        cursor.execute("CREATE PROCEDURE sqlaero_typed_change_context(IN input_value VARCHAR(128)) "
                       "BEGIN INSERT INTO typed_context_probe(value) VALUES(input_value); "
                       "SET SESSION sql_mode='NO_BACKSLASH_ESCAPES'; END")
        try:
            cursor.execute("SET SESSION sql_mode='' ")
            cursor.execute("CALL sqlaero_typed_change_context(%s)", source, many=True)
            cursor.execute("SELECT value FROM typed_context_probe ORDER BY id")
            assert cursor.fetchall() == tuple((value,) for value in values)
        finally:
            cursor.execute("DROP PROCEDURE IF EXISTS sqlaero_typed_change_context")
