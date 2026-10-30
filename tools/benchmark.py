"""Collect adjacent, balanced full-call timings with restartable JSONL evidence.

This is the project's single sampling entry. Inputs and outputs are checked
once outside timing; each timed result has its normal allocation/destruction.
Native escape output remains owned by Rust. NumPy is used only after collection
for the predeclared process-cluster bootstrap, never by a product invocation.
"""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass, replace
from datetime import date, datetime, time as daytime, timedelta, timezone
from decimal import Decimal
import gc
import hashlib
import importlib
import importlib.util
from importlib.machinery import EXTENSION_SUFFIXES
import json
import math
import os
from pathlib import Path
import pickle
import platform
import statistics
import subprocess
import sys
import time
from typing import Any, Callable, TextIO, cast

# Formal resource snapshots are a macOS responsibility. The CI smoke path
# below only needs portable wall timing and must also import on Windows.
resource: Any = None if os.name == 'nt' else importlib.import_module('resource')

ROOT = Path(__file__).resolve().parents[1]
FROZEN = ROOT / "experimental/rust/list_rust.so"
Call = Callable[[], object]
CPU_FAMILIES = frozenset({"escape", "bind", "fetch", "decode-json", "diagnostic"})


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fingerprint(value: object) -> dict[str, object]:
    """Inspect the final result outside timing, without retaining sample values."""
    original_type = type(value).__qualname__
    if hasattr(value, "to_bytes") and original_type == "_EncodedValue":
        value = cast(Any, value).to_bytes()
    if original_type == "Connection":
        connection = cast(Any, value)
        value = (connection.server_version, connection.tls_version)
    if original_type == "ErrorInfo":
        info = cast(Any, value)
        value = (info.stage, info.message, info.code, info.is_server_error,
                 info.notes, len(info.additional_errors))
    payload = pickle.dumps(value, protocol=5)
    result: dict[str, object] = {"type": original_type, "sha256": hashlib.sha256(payload).hexdigest(),
                                 "size": len(payload)}
    if isinstance(value, (str, bytes)):
        encoded = value.encode("utf8", "surrogatepass") if isinstance(value, str) else value
        result.update(sql_utf8_sha256=hashlib.sha256(encoded).hexdigest(), value_bytes=len(encoded))
    return result


@dataclass(frozen=True)
class Case:
    id: str
    value: object
    family: str = "escape"
    sql: str = ""
    many: bool = False
    itemize: bool = True
    no_backslash: bool = False

    def identity(self) -> dict[str, object]:
        return {"id": self.id, "family": self.family, "input": fingerprint(self.value),
                "sql": self.sql, "many": self.many, "itemize": self.itemize,
                "no_backslash": self.no_backslash}


@dataclass
class Task:
    call: Call
    prepare: Callable[[int], None] | None = None
    prepare_each_role: bool = False
    finish: Callable[[], None] | None = None


def cases() -> list[Case]:
    """Build the same complete input collection once in every real worker."""
    values: dict[str, list[object]] = {
        "none": [None], "bool": [True, False],
        "int": [7, -123456789, 2**63 - 1, 10**64 - 1],
        "float": [0.1, -0.0, 1.7976931348623157e308, float("nan"), float("inf")],
        "decimal": [Decimal("123.4500"), Decimal("-0.000001"), Decimal("NaN")],
        "str": ["abcdefghijklmnop", "a'\"\\\n\r\x00\x1a", "中文🙂é"],
        "bytes": [b"abcdefghijklmnop", b"a'\\\x00\xff"],
        "bytearray": [bytearray(b"abcdefghijklmnop"), bytearray(b"\x00\xff'")],
        "date": [date(2024, 2, 29), date(1, 1, 1)],
        "datetime": [datetime(2024, 2, 29, 12, 34, 56, 123456),
                     datetime(2024, 2, 29, 12, 34, tzinfo=timezone(timedelta(hours=8)))],
        "time": [daytime(12, 34, 56, 123456), daytime(23, 59, 59)],
        "timedelta": [timedelta(days=-3, microseconds=123), timedelta(days=10000)],
    }
    result: list[Case] = []
    for kind, variants in values.items():
        for index, value in enumerate(variants):
            result.append(Case(f"escape/{kind}/v{index}/scalar", value))
        base = variants[0]
        for size in (1, 100, 1000, 10000):
            for shape in ("list", "tuple"):
                for variant in ("same", "varied"):
                    if variant == "varied" and (size == 1 or len(variants) == 1):
                        continue
                    sequence = [base if variant == "same" else variants[i % len(variants)]
                                for i in range(size)]
                    data: object = sequence if shape == "list" else tuple(sequence)
                    result.append(Case(f"escape/{kind}/{variant}/{shape}/N{size}", data))
    for size in (1, 100, 1000, 10000):
        result.append(Case(f"escape/mixed/list/N{size}",
                           [i if i % 3 == 0 else "中'" if i % 3 == 1 else None for i in range(size)]))
        result.append(Case(f"escape/nested/list/N{size}", [(i, ["x", None]) for i in range(size)]))
        for shape in ("list", "tuple"):
            data = list(range(size)) if shape == "list" else tuple(range(size))
            result.append(Case(f"bind/positions/{shape}/N{size}", data, "bind",
                               "SELECT " + ",".join(["%s"] * size)))
            result.append(Case(f"bind/value/{shape}/N{size}", data, "bind", "SELECT %s", itemize=False))
        rows = [(i, "中文'", bool(i % 2)) for i in range(size)]
        result.append(Case(f"bind/insert/rows/N{size}", rows, "bind",
                           "INSERT INTO t(a,b,c) VALUES (%s,%s,%s)", True))
        result.append(Case(f"bind/fallback/rows/N{size}", rows, "bind",
                           "SELECT %s,%s,%s", True))
    for size in (1, 100, 1000, 10000):
        for variant, text in (("ascii", "a"), ("unicode", "中🙂"), ("special", "'\\\x00")):
            result.append(Case(f"escape/str/{variant}/codepoints{size}", (text * size)[:size]))
        for kind in ("bytes", "bytearray"):
            for variant, content in (("ascii", b"a"), ("special", b"'\\\x00\xff")):
                payload = (content * size)[:size]
                result.append(Case(f"escape/{kind}/{variant}/bytes{size}",
                                   payload if kind == "bytes" else bytearray(payload)))
    result.extend([
        Case("escape/empty/list", []), Case("escape/empty/tuple", ()),
        Case("escape/str/no-backslash/scalar", "a'\\\x00", no_backslash=True),
        Case("bind/empty/many", [], "bind", "SELECT 1", True),
        Case("bind/nested/itemize-false", [([1, 2],), ([3, 4],)], "bind", "SELECT %s", True, False),
        Case("bind/protected/positions", [1, "x"], "bind", "SELECT '%s',%s,%%,/* %s */%s"),
    ])
    for server in ("84", "97"):
        for transport in ("unix", "tcp"):
            for tls in (("disabled",) if transport == "unix" else ("disabled", "verified")):
                for auth in ("cached", "full"):
                    result.append(Case(f"connect/{server}/{transport}/{tls}/{auth}",
                                       {"server": server, "transport": transport, "tls": tls,
                                        "auth": auth}, "connect"))
            for size in (1, 100, 1000, 10000):
                result.append(Case(f"execute/{server}/{transport}/select/R{size}C3",
                                   {"server": server, "transport": transport, "rows": size}, "execute"))
        for size in (1, 100, 1000, 10000):
            for schema in ("numeric", "text", "mixed", "wide", "json", "decimal", "decimal-float", "temporal", "options"):
                for method in ("fetchone", "fetchmany", "fetchall"):
                    # fetchone consumes one row; large-R variants would repeat
                    # the same output task and are therefore not duplicated.
                    if method == "fetchone" and size != 1:
                        continue
                    result.append(Case(f"fetch/{server}/{schema}/{method}/R{size}",
                        {"server": server, "transport": "unix", "rows": size,
                         "schema": schema, "method": method}, "fetch"))
        for size in (1, 100, 1000, 10000):
            result.append(Case(f"execute/{server}/unix/batch/N{size}",
                {"server": server, "transport": "unix", "rows": size}, "batch"))
            result.append(Case(f"execute/{server}/unix/fallback/N{size}",
                {"server": server, "transport": "unix", "rows": size}, "fallback"))
            for schema in ("mixed", "wide"):
                result.append(Case(f"execute/{server}/unix/roundtrip/{schema}/R{size}",
                    {"server": server, "transport": "unix", "rows": size, "schema": schema}, "roundtrip"))
    # Exercise the repaired single-token work path using the existing paired
    # runner. These inputs are prepared once, outside normal call timing.
    for name, payload in (
        ("ascii-small", b'"' + b'a' * 1024 + b'"'),
        ("ascii-large", b'"' + b'a' * 1048576 + b'"'),
        ("unicode-large", ('"' + 'é中😀' * 65536 + '"').encode()),
        ("escapes-large", b'"' + b'\\u0001' * 65536 + b'"'),
        ("first-key", b'{"' + b'k' * 32768 + b'":1}'),
        ("later-key", b'{"first":0,"' + b'k' * 32768 + b'":1}'),
    ):
        result.append(Case(f"decode-json/{name}", payload, "decode-json"))
    # The new snapshot/GC boundary needs both cold construction and reused
    # properties. These public calls use the same timer, pairing and quality
    # rules as every other CPU-bound task; no separate sampler is introduced.
    for mode in ("snapshot-cold", "snapshot-cached", "notes-cached", "additional-cached",
                 "code-small", "code-large"):
        result.append(Case(f"diagnostic/{mode}", mode, "diagnostic"))
    for size in (262145, 1048576):
        result.append(Case(f"escape/bytearray/snapshot/bytes{size}", bytearray(b"a" * size)))
    return result


def load_native(package: str | None) -> Any:
    if package:
        sys.path.insert(0, str(Path(package).resolve()))
    return importlib.import_module("sqlaero._sqlaero")


def load_legacy() -> Any:
    spec = importlib.util.spec_from_file_location("list_rust", FROZEN)
    if spec is None or spec.loader is None:
        raise RuntimeError("Frozen v23 native loader is unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def target(case: Case, native: Any, legacy: Any | None, sessions: dict[str, Any]) -> Task:
    if legacy is not None:
        if case.family != "escape" or case.no_backslash:
            raise ValueError("v23 whole-value boundary does not implement this task")
        escape = legacy.escape_common
        return Task(lambda: escape(case.value))
    if case.family == "escape":
        encode = native._prepare_encoder(no_backslash=case.no_backslash).encode
        return Task(lambda: encode(case.value))
    if case.family == "bind":
        bind = native._bind_sql
        return Task(lambda: bind(case.sql, case.value, many=case.many,
                                 itemize=case.itemize, no_backslash=case.no_backslash))
    if case.family == "decode-json":
        decode = native._decode_json
        return Task(lambda: decode(case.value))
    if case.family == "diagnostic":
        error_type = native.SQLAeroError
        if case.value == "snapshot-cold":
            return Task(lambda: error_type("benchmark diagnostic").error_info)
        code = 1 << 200 if case.value == "code-large" else 65537
        error = error_type("benchmark diagnostic", code=code, is_server_error=False)
        info = error.error_info
        if case.value == "snapshot-cached":
            return Task(lambda: error.error_info)
        if case.value == "notes-cached":
            return Task(lambda: info.notes)
        if case.value == "additional-cached":
            info.additional_errors
            return Task(lambda: info.additional_errors)
        return Task(lambda: error.code)
    return network_target(case, native, sessions)


def network_target(case: Case, native: Any, sessions: dict[str, Any]) -> Task:
    """Use real public API calls; query preparation is never hidden in fetch."""
    spec = cast(dict[str, Any], case.value)
    fixture = Path(os.environ.get("SQLAERO_TEST_MYSQL_ROOT", "/private/tmp/sqlaero-mysql-501"))
    label, transport = str(spec["server"]), str(spec["transport"])
    secret = json.loads((fixture / label / "state.json").read_text())
    settings: dict[str, Any] = {"user": "sqlaero", "password": secret["test_password"],
                                "database": "sqlaero_test", "operation_timeout": 10}
    if transport == "unix":
        settings["unix_socket"] = fixture / f"{label}.sock"
    else:
        settings.update(host="127.0.0.1", port=int(secret.get("port", 13384 if label == "84" else 13397)))
        settings["ssl"] = native.SSL(mode="DISABLED")
        settings["server_public_key_path"] = fixture / label / "data/public_key.pem"
    if spec.get("tls") == "verified":
        settings["ssl"] = native.SSL(mode="VERIFY_IDENTITY", ca_file=fixture / "tls/ca.pem",
                                     server_name="localhost")
    if case.family == "connect":
        created: list[Any] = []

        def connect_ready() -> object:
            connection = native.connect(**settings)
            created.append(connection)
            return connection

        def close_created() -> None:
            for connection in created:
                connection.close()
            created.clear()

        if spec["auth"] == "cached":
            return Task(connect_ready, finish=close_created)
        key = f"admin/{label}"
        if key not in sessions:
            sessions[key] = native.connect(user="root", password=secret["root_password"],
                                           unix_socket=fixture / f"{label}.sock")
        admin = sessions[key].cursor()

        def clear_auth_cache(n: int) -> None:
            if n != 1:
                raise ValueError("Full-auth connection samples require n=1 and one untimed cache reset")
            admin.execute("FLUSH PRIVILEGES")

        return Task(connect_ready, clear_auth_cache, True, close_created)
    key = f"data/{label}/{transport}"
    if key not in sessions:
        connection = native.connect(**settings)
        cursor = connection.cursor()
        cursor.execute("CREATE TEMPORARY TABLE sqlaero_bench(n INT PRIMARY KEY, s VARCHAR(64), d DOUBLE, b BIT(8), choices SET('a','b'))")
        cursor.execute("INSERT INTO sqlaero_bench VALUES (%s,%s,%s,%s,%s)",
                       [(i, f"row{i}中", i + 0.125, i % 256, "a,b") for i in range(10000)], many=True)
        sessions[key] = connection
    connection = sessions[key]
    connection.decode_json = False
    connection.decode_set = False
    connection.decode_bit = False
    connection.use_decimal = True
    connection.use_timedelta = True
    cursor = connection.cursor()
    size = int(spec["rows"])
    if case.family in {"batch", "fallback"}:
        cursor.execute("CREATE TEMPORARY TABLE IF NOT EXISTS sqlaero_write(n INT,s VARCHAR(64))")
        args = [(i, "value中") for i in range(size)]

        def reset_write(_: int) -> None:
            cursor.execute("TRUNCATE sqlaero_write")

        sql = "INSERT INTO sqlaero_write VALUES (%s,%s)"
        if case.family == "fallback":
            sql += " ON DUPLICATE KEY UPDATE s=CONCAT(s,%s)"
            # A suffix placeholder makes this legal per-row execution, not a
            # merge candidate. Every row still inserts one complete record.
            args = [(i, "value中", "suffix") for i in range(size)]
        return Task(lambda: cursor.execute(sql, args, many=True), reset_write)
    schema = str(spec.get("schema", "mixed"))
    projections = {"numeric": "n,d", "text": "s", "mixed": "n,s,d,DATE'2024-02-29'",
                   "wide": ",".join(f"{('n', 'd', 's')[i % 3]} AS c{i}" for i in range(32)),
                   "json": "JSON_OBJECT('n',n,'s',s)",
                   "decimal": "CAST(d AS DECIMAL(20,4))", "decimal-float": "CAST(d AS DECIMAL(20,4)),d",
                   "temporal": "DATE'2024-02-29',TIMESTAMP'2024-02-29 12:34:56.123456',CAST('12:34:56.123456' AS TIME(6))",
                   "options": "b,choices,JSON_OBJECT('n',n),CAST('12:34:56' AS TIME)"}
    sql = f"SELECT {projections[schema]} FROM sqlaero_bench WHERE n < %s ORDER BY n"
    if case.family == "execute":
        return Task(lambda: cursor.execute(sql, size))
    if case.family == "roundtrip":
        def execute_fetch() -> object:
            cursor.execute(sql, size)
            return cursor.fetchall()

        return Task(execute_fetch)
    if schema == "json":
        connection.decode_json = True
    if schema == "decimal-float":
        connection.use_decimal = False
    if schema == "options":
        connection.decode_bit = True
        connection.decode_set = True
        connection.decode_json = True
        connection.use_timedelta = False
    pool: list[Any] = []
    position = 0
    method = str(spec["method"])

    def release_fetch() -> None:
        for old in pool:
            old.close()
        pool.clear()

    def prepare_fetch(n: int) -> None:
        nonlocal position
        release_fetch()
        position = 0
        for _ in range(n):
            current = connection.cursor()
            current.execute(sql, size)
            pool.append(current)

    def fetch() -> object:
        nonlocal position
        current = pool[position]
        position += 1
        if method == "fetchone":
            return current.fetchone()
        if method == "fetchmany":
            return current.fetchmany(size)
        return current.fetchall()

    return Task(fetch, prepare_fetch, finish=release_fetch)


def clock_calibration() -> dict[str, object]:
    """Record an empirical boundary allowance, not a universal error bound."""
    spans: list[int] = []
    differences: list[int] = []
    wall_reads: list[int] = []
    for _ in range(2000):
        outer0 = time.perf_counter_ns()
        cpu0 = time.thread_time_ns()
        wall0 = time.perf_counter_ns()
        wall1 = time.perf_counter_ns()
        cpu1 = time.thread_time_ns()
        outer1 = time.perf_counter_ns()
        spans.append((outer1 - outer0) - (wall1 - wall0))
        wall_reads.append(wall1 - wall0)
        differences.append(abs((cpu1 - cpu0) - (wall1 - wall0)))
    resolution = max(time.get_clock_info("perf_counter").resolution,
                     time.get_clock_info("thread_time").resolution)
    # A conservative observed margin plus 10 us slack. It is rechecked against
    # the actual batch boundary, and insufficient evidence is never zero load.
    margin = max(max(spans), max(differences), math.ceil(resolution * 1e9)) + 10000
    wall_reads.sort()
    timer_allowance = wall_reads[math.ceil(len(wall_reads) * 0.999) - 1] * 2 + math.ceil(resolution * 1e9)
    return {"margin_ns": margin, "max_snapshot_ns": max(spans),
            "max_empty_difference_ns": max(differences), "observations": len(spans),
            "resolution_ns": resolution * 1e9,
            "timer_read_allowance_ns": timer_allowance,
            "timer_read_allowance_source": "twice observed p99.9 back-to-back wall reads plus resolution; empirical",
            "nature": "empirical observed allowance plus 10us; not an absolute bound"}


def measure(call: Call, n: int) -> dict[str, object]:
    """Time full calls; keep normal allocation, replacement, GC and cleanup."""
    usage_scope = cast(int, getattr(resource, "RUSAGE_THREAD", resource.RUSAGE_SELF))
    before = resource.getrusage(usage_scope)
    outer0 = time.perf_counter_ns()
    cpu0 = time.thread_time_ns()
    wall0 = time.perf_counter_ns()
    for _ in range(n):
        # Discard each fully formed output immediately, as timeit does for a
        # call expression. Retaining the previous large fetch result while
        # building the next changes the live GC graph and peak memory.
        call()
    wall1 = time.perf_counter_ns()
    cpu1 = time.thread_time_ns()
    outer1 = time.perf_counter_ns()
    after = resource.getrusage(usage_scope)
    return {"n": n, "wall_ns": wall1 - wall0, "cpu_ns": cpu1 - cpu0,
            "wall_start_ns": wall0, "wall_end_ns": wall1,
            "cpu_start_ns": cpu0, "cpu_end_ns": cpu1,
            "snapshot_ns": (outer1 - outer0) - (wall1 - wall0),
            "voluntary_switches": after.ru_nvcsw - before.ru_nvcsw,
            "involuntary_switches": after.ru_nivcsw - before.ru_nivcsw,
            "switch_scope": "thread" if hasattr(resource, "RUSAGE_THREAD") else "process diagnostic"}


def worker(package: str | None, with_legacy: bool, warmup_calls: int = 16) -> None:
    native = load_native(package)
    legacy = load_legacy() if with_legacy else None
    collection = {case.id: case for case in cases()}
    calls: dict[tuple[str, str], Task] = {}
    preparations: dict[tuple[str, str], dict[str, int]] = {}
    sessions: dict[str, Any] = {}
    outputs: dict[str, object] = {}
    warmups: dict[tuple[str, str], dict[str, int]] = {}
    calibration = clock_calibration()
    ready = {"pid": os.getpid(), "start_unix_ns": time.time_ns(), "python": sys.version, "gc_enabled": gc.isenabled(),
             "module": native.__file__, "artifact_sha256": digest(Path(native.__file__)),
             "clock": calibration, "inputs": [case.identity() for case in collection.values()],
             "legacy_sha256": digest(FROZEN) if with_legacy else None,
             "warmup_calls": warmup_calls, "warmup_wall_cap_ns": 2_000_000}
    print(json.dumps({"ready": ready}), flush=True)
    for line in sys.stdin:
        request = cast(dict[str, Any], json.loads(line))
        try:
            case_id, role = str(request["case"]), str(request.get("role", "native"))
            key = case_id, role
            if key not in calls:
                case = collection[case_id]
                task = target(case, native, legacy if role == "legacy" else None, sessions)
                if task.prepare:
                    task.prepare(1)
                actual = task.call()
                outputs["/".join(key)] = fingerprint(actual)
                if case.family == "escape" and role != "legacy":
                    expected = native._bind_sql("SELECT %s", case.value, itemize=False,
                                                 no_backslash=case.no_backslash)[0][8:]
                    if cast(Any, actual).to_bytes() != expected:
                        raise AssertionError("Measurement and production binding disagree")
                del actual
                if task.finish:
                    task.finish()
                # A cap of one fixes equal allocation work on both sides,
                # avoiding speed-dependent warmup histories before timed GC.
                # The existing 2ms wall cap and actual cost remain recorded.
                start, count = time.perf_counter_ns(), 0
                while count < warmup_calls and time.perf_counter_ns() - start < 2_000_000:
                    if task.prepare:
                        task.prepare(1)
                    task.call()
                    if task.finish:
                        task.finish()
                    count += 1
                warmups[key] = {"calls": count, "max_calls": warmup_calls,
                                "elapsed_ns": time.perf_counter_ns() - start}
                calls[key] = task
            task = calls[key]
            if request.get("timing_control"):
                n = int(request["n"])
                if task.prepare is None or collection[case_id].family != "fetch":
                    raise ValueError("Matched-work timing control requires a consumptive fetch task")
                task.prepare(2 * n)
                outer_start = time.perf_counter_ns()
                first = measure(task.call, n)
                second = measure(task.call, n)
                outer_wall = time.perf_counter_ns() - outer_start
                if task.finish:
                    task.finish()
                for inner in (first, second):
                    inner.update(clock=calibration, cpu_quality=True)
                print(json.dumps({"outer_wall_ns": outer_wall, "samples": [first, second]}), flush=True)
                continue
            if request.get("prepare"):
                if task.prepare and bool(request.get("each_role")) == task.prepare_each_role:
                    preparation_start = time.perf_counter_ns()
                    task.prepare(int(request["n"]))
                    preparations[key] = {"n": int(request["n"]),
                        "elapsed_ns": time.perf_counter_ns() - preparation_start,
                        "process_maxrss": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss}
                print(json.dumps({"prepared": request["n"], "pid": os.getpid()}), flush=True)
                continue
            result = measure(task.call, int(request["n"]))
            if task.finish:
                cleanup_start = time.perf_counter_ns()
                task.finish()
                result["untimed_cleanup_ns"] = time.perf_counter_ns() - cleanup_start
            result.update({"pid": os.getpid(), "case": case_id, "role": role,
                           "output": outputs["/".join(key)], "clock": calibration})
            result["untimed_preparation"] = preparations.get(key)
            result["warmup"] = warmups[key]
            result["cpu_quality"] = collection[case_id].family in CPU_FAMILIES
            print(json.dumps(result), flush=True)
        except BaseException as error:
            print(json.dumps({"error": type(error).__name__, "message": str(error),
                              "case": request.get("case"), "role": request.get("role")}), flush=True)
    for connection in sessions.values():
        connection.close()


class Worker:
    def __init__(self, package: str | None, legacy: bool, log: TextIO,
                 *, warmup_calls: int = 16) -> None:
        command = [sys.executable, str(Path(__file__).resolve()), "worker",
                   "--warmup-calls", str(warmup_calls)]
        if package:
            command.extend(["--package", package])
        if legacy:
            command.append("--legacy")
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=log, text=True, bufsize=1)
        self.ready = self.read()

    def read(self) -> dict[str, Any]:
        assert self.process.stdout is not None
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f"Worker {self.process.pid} ended: {self.process.poll()}")
        result = cast(dict[str, Any], json.loads(line))
        if "error" in result:
            raise RuntimeError(f"Worker call failed: {result}")
        return result

    def sample(self, case_id: str, n: int, role: str) -> dict[str, Any]:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps({"case": case_id, "n": n, "role": role}) + "\n")
        self.process.stdin.flush()
        return self.read()

    def prepare(self, case_id: str, n: int, role: str, *, each_role: bool = False) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps({"case": case_id, "n": n, "role": role,
                                            "prepare": True, "each_role": each_role}) + "\n")
        self.process.stdin.flush()
        self.read()

    def timing_control(self, case_id: str, n: int, role: str) -> dict[str, Any]:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps({"case": case_id, "n": n, "role": role,
                                            "timing_control": True}) + "\n")
        self.process.stdin.flush()
        return self.read()

    def close(self) -> None:
        if self.process.stdin is not None:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=5)


def calibrated_sample(worker: Worker, case: Case, n: int, role: str) -> dict[str, Any]:
    worker.prepare(case.id, n, role, each_role=case.family == "connect")
    return worker.sample(case.id, n, role)


def control(worker: Worker, case: Case, n: int, role: str) -> dict[str, object]:
    """Check normalized short/long timings in both length orders, untuned by speed."""
    samples: list[dict[str, Any]] = []
    ratios: list[float] = []
    if case.family == "fetch":
        # Each Cursor can be consumed only once. Varying the batch also varies
        # the live result graph and cache working set. Compare nested timings
        # of the *same* 2n full calls instead of treating that real work as
        # clock error. Allocation, destruction and timed GC remain included.
        observations = [worker.timing_control(case.id, n, role) for _ in range(2)]
        for observation in observations:
            inner = cast(list[dict[str, Any]], observation["samples"])
            samples.extend(inner)
            ratios.append(int(observation["outer_wall_ns"]) / sum(int(item["wall_ns"]) for item in inner))
        normalized_ratio = statistics.median(ratios)
        reasons = [quality(sample, 1_000_000) for sample in samples]
        return {"role": role, "kind": "nested_same_complete_work", "samples": samples,
                "observations": observations, "normalized_ratio": normalized_ratio,
                "quality_reasons": reasons,
                "passed": not any(reasons) and abs(normalized_ratio - 1) <= 0.05}
    for order in ((1, 2), (2, 1), (1, 2), (2, 1)):
        pair: dict[int, dict[str, Any]] = {}
        for multiple in order:
            pair[multiple] = calibrated_sample(worker, case, n * multiple, role)
        samples.extend(pair.values())
        short, long = pair[1], pair[2]
        ratios.append((int(long["wall_ns"]) / (2 * n)) / (int(short["wall_ns"]) / n))
    normalized_ratio = statistics.median(ratios)
    reasons = [quality(sample, 1_000_000) for sample in samples]
    return {"role": role, "samples": samples, "normalized_ratio": normalized_ratio,
            "quality_reasons": reasons, "passed": not any(reasons) and abs(normalized_ratio - 1) <= 0.05}
def quality(sample: dict[str, Any], minimum_ns: int) -> str | None:
    wall, cpu = int(sample["wall_ns"]), int(sample["cpu_ns"])
    margin = int(sample["clock"]["margin_ns"])
    if wall <= 0 or cpu < 0:
        raise RuntimeError("Invalid mandatory timer counter")
    if not sample["cpu_quality"]:
        return None
    if wall < minimum_ns:
        return "short_batch"
    if int(sample["clock"]["timer_read_allowance_ns"]) / wall > 0.01:
        return "wall_timer_allowance_above_one_percent"
    if int(sample["snapshot_ns"]) > margin:
        return "snapshot_outside_calibration"
    if cpu - wall > margin:
        return "negative_difference_outside_calibration"
    if (wall - cpu + margin) / wall > 0.1:
        return "off_cpu_proxy"
    return None


def append(path: Path, value: object) -> None:
    with path.open("a") as stream:
        stream.write(json.dumps(value, sort_keys=True) + "\n")


def package_identity(package: str | None) -> dict[str, object]:
    folder = Path(package).resolve() / "sqlaero" if package else ROOT / "python/sqlaero"
    artifacts = sorted(path for path in folder.iterdir() if path.name.startswith('_sqlaero')
                       and path.name.endswith(tuple(EXTENSION_SUFFIXES)))
    if len(artifacts) != 1:
        raise RuntimeError(f"Expected exactly one native artifact in {folder}")
    return {"package": str(folder), "artifact": str(artifacts[0]),
            "sha256": digest(artifacts[0]),
            "python_files": {str(path.relative_to(folder)): digest(path)
                             for path in sorted(folder.glob("*.py"))}}


def collect(args: argparse.Namespace) -> None:
    selected = [case for case in cases() if case.id in args.case or
                any(case.id.startswith(prefix) for prefix in args.prefix)]
    if args.legacy:
        selected = [case for case in selected if case.family == "escape" and not case.no_backslash]
    missing = set(args.case) - {case.id for case in selected}
    if missing:
        raise ValueError(f"Unknown or inapplicable case IDs: {sorted(missing)}")
    if not selected:
        raise ValueError("No applicable cases selected")
    directory = Path(args.output).resolve()
    directory.mkdir(parents=True, exist_ok=True)
    parameters: dict[str, int] = {}
    if args.config:
        config = cast(dict[str, Any], json.loads(Path(args.config).read_text()))
        parameters = {str(key): int(value) for key, value in config["calls"].items()}
        if "warmup_calls" in config and config["warmup_calls"] != args.warmup_calls:
            raise ValueError("Warmup call cap differs from the frozen configuration")
    started = time.perf_counter_ns()
    deadline = started + int(args.seconds * 1e9)
    role_a = "legacy" if args.legacy else "native"
    records_path = directory / "groups.jsonl"
    existing: set[tuple[int, str, str]] = set()
    contract = {"runner_sha256": digest(Path(__file__)), "phase": args.command,
                "python": sys.version, "machine": platform.machine(),
                "A": {"legacy_sha256": digest(FROZEN)} if args.legacy else package_identity(args.reference),
                "B": package_identity(args.candidate), "cycles": args.cycles,
                "minimum_ms": args.minimum_ms, "target_ms": args.target_ms,
                "warmup_calls": args.warmup_calls, "warmup_wall_cap_ns": 2_000_000,
                "config_sha256": digest(Path(args.config)) if args.config else None,
                "cases": [case.identity() for case in selected]}
    contract_path = directory / "contract.json"
    if contract_path.exists() and json.loads(contract_path.read_text()) != contract:
        raise ValueError("Collection identity changed; preserve this run and use a new data directory")
    contract_path.write_text(json.dumps(contract, indent=2))
    if records_path.exists():
        slots: dict[tuple[int, str, str], set[str]] = {}
        for line in records_path.read_text().splitlines():
            row = json.loads(line)
            if row["run_sha256"] != contract["runner_sha256"]:
                raise ValueError("Runner identity differs from saved groups")
            key = int(row["cycle"]), str(row["case"]), str(row["cycle_key"])
            slots.setdefault(key, set()).add(str(row["order"]))
        for (cycle, case_id, _), orders in slots.items():
            if orders == {"AB", "BA"}:
                existing.update((cycle, case_id, order) for order in orders)
        # A partial real process cycle remains raw evidence. It is not joined
        # to a different PID; repeat its complete two-order cluster instead.
        if args.command == "calibrate" and (directory / "config.json").exists():
            saved = json.loads((directory / "config.json").read_text())
            parameters.update({str(key): int(value) for key, value in saved["calls"].items()})
    metadata = {"runner_sha256": digest(Path(__file__)), "phase": args.command,
                "python": sys.version, "platform": platform.platform(), "machine": platform.machine(),
                "start_unix_ns": time.time_ns(), "seconds": args.seconds, "cycles": args.cycles,
                "warmup_calls": args.warmup_calls, "warmup_wall_cap_ns": 2_000_000,
                "reference_package": args.reference, "candidate_package": args.candidate,
                "legacy": args.legacy, "cases": [case.identity() for case in selected],
                "boundary": "full production visitor to Rust owner; bind probe includes Python diagnostic copies",
                "gc": "enabled; normal timed GC retained", "bootstrap_seed": 20261026}
    append(directory / "invocations.jsonl", metadata)
    rejections: Counter[str] = Counter()
    isolated: set[str] = set()
    completed = 0
    with (directory / "workers.log").open("a") as log:
        for cycle in range(args.cycles):
            needed = [case for case in selected if any((cycle, case.id, order) not in existing
                                                       for order in ("AB", "BA"))]
            if not needed:
                continue
            a: Worker | None = None
            b: Worker | None = None
            try:
                # Balance actual process/module load order across true cycles.
                if cycle % 2 == 0 or args.legacy:
                    a = Worker(args.reference, args.legacy, log, warmup_calls=args.warmup_calls)
                    b = a if args.legacy else Worker(args.candidate, False, log, warmup_calls=args.warmup_calls)
                else:
                    b = Worker(args.candidate, False, log, warmup_calls=args.warmup_calls)
                    a = Worker(args.reference, False, log, warmup_calls=args.warmup_calls)
                append(directory / "processes.jsonl", {"cycle": cycle, "A": a.ready, "B": b.ready})
                actual_a = a.ready["ready"]["legacy_sha256"] if args.legacy else a.ready["ready"]["artifact_sha256"]
                expected_a = cast(dict[str, Any], contract["A"])
                expected_b = cast(dict[str, Any], contract["B"])
                if actual_a != expected_a["legacy_sha256" if args.legacy else "sha256"] or b.ready["ready"]["artifact_sha256"] != expected_b["sha256"]:
                    raise RuntimeError("Actually loaded artifact differs from the frozen contract")
                if any(current.ready["ready"]["warmup_calls"] != args.warmup_calls for current in (a, b)):
                    raise RuntimeError("Worker warmup call cap differs from the frozen contract")
                cycle_key = f"{cycle}/{a.process.pid}/{b.process.pid}/{a.ready['ready']['start_unix_ns']}"
                for case in needed:
                    if case.id in isolated:
                        continue
                    # Finish first-call checks/warmup before an adjacent pair.
                    # Counterbalance first-use/database creation as well as
                    # complete-block edge positions across true cycles.
                    orders = ("AB", "BA") if cycle % 2 == 0 else ("BA", "AB")
                    checked: dict[str, dict[str, Any]] = {}
                    for role in orders[0]:
                        checked[role] = (a if role == "A" else b).sample(
                            case.id, 0, role_a if role == "A" else "native")
                    checked_a, checked_b = checked["A"], checked["B"]
                    if not args.legacy and checked_a["output"] != checked_b["output"]:
                        raise RuntimeError(f"Reference and candidate outputs differ: {case.id}")
                    if case.id not in parameters:
                        if args.command == "run":
                            raise ValueError(f"Formal calls not frozen for {case.id}")
                        n = 1
                        probes: list[dict[str, Any]] = []
                        for _ in range(6):
                            sa = calibrated_sample(a, case, n, role_a)
                            sb = calibrated_sample(b, case, n, "native")
                            probes.extend((sa, sb))
                            fastest = min(int(sa["wall_ns"]), int(sb["wall_ns"]))
                            if case.family not in CPU_FAMILIES or fastest >= args.target_ms * 1e6 or n >= 10_000_000:
                                break
                            n = min(10_000_000, max(n + 1, math.ceil(n * args.target_ms * 1e6 / max(fastest, 1))))
                        parameters[case.id] = n
                        append(directory / "calibration.jsonl", {"case": case.id, "n": n, "probes": probes})
                        (directory / "config.json").write_text(json.dumps({"calls": parameters,
                            "cycles": args.cycles, "target_ms": args.target_ms,
                            "warmup_calls": args.warmup_calls,
                            "minimum_ms": args.minimum_ms, "runner_sha256": metadata["runner_sha256"],
                            "cases": metadata["cases"]}, indent=2))
                    n = parameters[case.id]
                    if args.command == "calibrate" and cycle == 0 and case.family in CPU_FAMILIES:
                        # A supplied call count can be reused, but its control
                        # evidence cannot be reused for a new native artifact.
                        controls = [control(a, case, n, role_a), control(b, case, n, "native")]
                        append(directory / "controls.jsonl", {"case": case.id, "n": n, "controls": controls})
                        if not all(item["passed"] for item in controls):
                            isolated.add(case.id)
                            continue
                    for order in orders:
                        key = cycle, case.id, order
                        if key in existing:
                            continue
                        consecutive = 0
                        while key not in existing:
                            if time.perf_counter_ns() >= deadline:
                                raise TimeoutError("Collection wall budget reached; complete groups saved")
                            # Balance real preparation recency as well as
                            # measurement position, especially for fetch pools.
                            for role in order:
                                (a if role == "A" else b).prepare(
                                    case.id, n, role_a if role == "A" else "native")
                            pair_start = time.perf_counter_ns()
                            samples: dict[str, dict[str, Any]] = {}
                            for role in order:
                                current = a if role == "A" else b
                                current.prepare(case.id, n, role_a if role == "A" else "native", each_role=True)
                                samples[role] = current.sample(
                                    case.id, n, role_a if role == "A" else "native")
                            span = time.perf_counter_ns() - pair_start
                            reasons = {role: quality(sample, int(args.minimum_ms * 1e6))
                                       for role, sample in samples.items()}
                            reason = next((str(item) for item in reasons.values() if item), None)
                            if span > (250_000_000 if samples["A"]["cpu_quality"] else 21_000_000_000):
                                reason = "group_span"
                            group = {"cycle": cycle, "case": case.id, "order": order,
                                     "cycle_key": cycle_key,
                                     "samples": samples, "span_ns": span, "reasons": reasons,
                                     "run_sha256": metadata["runner_sha256"]}
                            if reason is None:
                                append(records_path, group)
                                existing.add(key)
                                completed += 1
                                consecutive = 0
                            else:
                                rejections[reason] += 1
                                append(directory / "rejected.jsonl", {**group, "reason": reason})
                                consecutive += 1
                                if consecutive >= 10:
                                    isolated.add(case.id)
                                    append(directory / "isolated.jsonl", {"case": case.id,
                                        "reason": reason, "scope": "current scene/configuration",
                                        "consecutive": consecutive})
                                    break
                                time.sleep(1)
                        if case.id in isolated:
                            break
                    (directory / "checkpoint.json").write_text(json.dumps({
                        "complete_groups": len(existing), "last": [cycle, case.id],
                        "rejections": dict(rejections), "elapsed_ns": time.perf_counter_ns() - started}, indent=2))
                print(json.dumps({"cycle_complete": cycle, "new_groups": completed}), flush=True)
            finally:
                if b is not None and b is not a:
                    b.close()
                if a is not None:
                    a.close()
    config_output = {"calls": parameters, "cycles": args.cycles,
                     "warmup_calls": args.warmup_calls,
                     "target_ms": args.target_ms, "minimum_ms": args.minimum_ms,
                     "runner_sha256": metadata["runner_sha256"], "cases": metadata["cases"]}
    (directory / "config.json").write_text(json.dumps(config_output, indent=2))
    print(json.dumps({"complete": not isolated, "groups": len(existing), "elapsed_ns": time.perf_counter_ns() - started,
                      "rejections": dict(rejections), "isolated": sorted(isolated)}), flush=True)


def summarize(directory: Path) -> None:
    """Resample whole real cycles, retaining both within-cycle order slots."""
    # Existing frozen development NumPy is a statistics-only dependency.
    sys.path.append(str(ROOT / "experimental/.runtime/python/lib/python3.13/site-packages"))
    np: Any = importlib.import_module("numpy")
    rows = [cast(dict[str, Any], json.loads(line))
            for line in (directory / "groups.jsonl").read_text().splitlines()]
    by_case: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        by_case.setdefault(str(row["case"]), []).append(row)
    report: list[dict[str, object]] = []
    for case_id, groups in by_case.items():
        clusters: dict[str, dict[str, float]] = {}
        a_values: list[float] = []
        b_values: list[float] = []
        for group in groups:
            a, b = group["samples"]["A"], group["samples"]["B"]
            av, bv = float(a["wall_ns"]) / int(a["n"]), float(b["wall_ns"]) / int(b["n"])
            clusters.setdefault(str(group["cycle_key"]), {})[str(group["order"])] = av / bv
        complete = [cluster for cluster in clusters.values() if set(cluster) == {"AB", "BA"}]
        complete_keys = {key for key, cluster in clusters.items() if set(cluster) == {"AB", "BA"}}
        for group in groups:
            if str(group["cycle_key"]) in complete_keys:
                a, b = group["samples"]["A"], group["samples"]["B"]
                a_values.append(float(a["wall_ns"]) / int(a["n"]))
                b_values.append(float(b["wall_ns"]) / int(b["n"]))
        if not complete:
            report.append({"case": case_id, "groups": len(groups), "cycles": 0,
                           "performance": "INCOMPLETE_PROCESS_CLUSTER"})
            continue
        matrix = np.asarray([[cluster["AB"], cluster["BA"]] for cluster in complete])
        ratios = matrix.reshape(-1)
        interval: list[float] | None = None
        if len(complete) >= 2:
            rng = np.random.default_rng(20261026)
            indices = rng.integers(0, len(complete), size=(20000, len(complete)))
            draws = np.median(matrix[indices].reshape(20000, -1), axis=1)
            interval = [float(item) for item in np.percentile(draws, [2.5, 97.5])]
        report.append({"case": case_id, "cycles": len(complete), "groups": len(groups),
                       "ratio_median": float(np.median(ratios)) if len(ratios) else None,
                       "ratio_ci95": interval,
                       "ratio_range": [float(np.min(ratios)), float(np.max(ratios))] if len(ratios) else None,
                       "reference_median_ns": statistics.median(a_values),
                       "candidate_median_ns": statistics.median(b_values),
                       "reference_mean_ns": statistics.mean(a_values),
                       "candidate_mean_ns": statistics.mean(b_values),
                       "quality": "VALID_GROUPS_ONLY", "performance": "NOT_AUTOMATICALLY_ACCEPTED"})
    (directory / "summary.json").write_text(json.dumps({"numpy": np.__version__, "seed": 20261026,
        "resamples": 20000, "cluster": "real process cycle, preserving AB and BA", "cases": report}, indent=2))
    print(json.dumps({"summarized": len(report), "path": str(directory / "summary.json")}))


def smoke(directory: Path, seconds: float) -> None:
    """Record five fixed wall batches for eight existing tasks, without filtering.

    CI uses this diagnostic only. It does not claim statistical stability or
    replace formal macOS paired measurements and their quality contract.
    """
    native = importlib.import_module('sqlaero._sqlaero')
    native_file = native.__file__
    if native_file is None:
        raise RuntimeError('The installed native extension has no file identity')
    fixed = {
        'escape/int/same/list/N1000': 200,
        'escape/str/same/list/N1000': 200,
        'bind/positions/list/N1000': 100,
        'bind/insert/rows/N1000': 100,
        'decode-json/ascii-small': 1000,
        'execute/84/tcp/select/R100C3': 5,
        'fetch/84/json/fetchall/R100': 10,
        'fetch/84/mixed/fetchall/R100': 10,
    }
    collection = {case.id: case for case in cases()}
    sessions: dict[str, Any] = {}
    results: list[dict[str, object]] = []
    started = time.monotonic()
    try:
        for case_id, count in fixed.items():
            case = collection[case_id]
            if case.family == 'fetch':
                case = replace(case, value={**cast(dict[str, Any], case.value), 'transport': 'tcp'})
            task = target(case, native, None, sessions)
            try:
                if task.prepare:
                    task.prepare(1)
                checked = fingerprint(task.call())
                if task.finish:
                    task.finish()
                batches: list[int] = []
                for _ in range(5):
                    if time.monotonic() - started >= seconds:
                        raise TimeoutError('the fixed CI smoke command exhausted its wall budget')
                    if task.prepare:
                        task.prepare(count)
                    before = time.perf_counter_ns()
                    for _ in range(count):
                        task.call()
                    batches.append(time.perf_counter_ns() - before)
                    if task.finish:
                        task.finish()
                results.append({'case': case_id, 'calls_per_batch': count,
                                'wall_ns': batches, 'output': checked})
            finally:
                if task.finish:
                    task.finish()
    finally:
        for connection in sessions.values():
            connection.close()
    directory.mkdir(parents=True, exist_ok=True)
    output = {'purpose': 'diagnostic only; no formal performance verdict',
              'python': sys.version, 'platform': platform.platform(), 'machine': platform.machine(),
              'native': native_file, 'native_sha256': digest(Path(native_file)),
              'elapsed_seconds': time.monotonic() - started, 'cases': results}
    (directory / 'performance-smoke.json').write_text(json.dumps(output, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(output))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("worker", "list", "calibrate", "run", "summarize", "smoke"))
    parser.add_argument("--output", default="evidence/rust-sync-kernel/performance/preflight")
    parser.add_argument("--package")
    parser.add_argument("--reference")
    parser.add_argument("--candidate")
    parser.add_argument("--legacy", action="store_true")
    parser.add_argument("--prefix", action="append", default=[])
    parser.add_argument("--case", action="append", default=[], help="Select an exact case ID without prefix expansion")
    parser.add_argument("--config")
    parser.add_argument("--cycles", type=int, default=12)
    parser.add_argument("--seconds", type=float, default=600)
    parser.add_argument("--target-ms", type=float, default=2)
    parser.add_argument("--minimum-ms", type=float, default=1)
    parser.add_argument("--warmup-calls", type=int, choices=range(1, 17), default=16,
                        help="Maximum warmup calls per scene; use 1 for equal fixed work before timed GC")
    args = parser.parse_args()
    if resource is None and args.command not in ('list', 'smoke'):
        raise RuntimeError('Formal resource-based performance measurement is supported on the macOS development host')
    if args.command == 'smoke':
        if not 0 < args.seconds <= 180:
            raise ValueError('CI smoke requires a positive budget of at most 180 seconds')
        smoke(Path(args.output), args.seconds)
    elif args.command == "worker":
        worker(args.package, args.legacy, args.warmup_calls)
    elif args.command == "list":
        print(json.dumps([case.identity() for case in cases()], indent=2))
    elif args.command == "summarize":
        summarize(Path(args.output))
    else:
        if not args.prefix and not args.case:
            args.prefix = [""]
        if args.cycles <= 0 or args.seconds <= 0 or args.minimum_ms <= 0 or args.target_ms <= 0:
            raise ValueError("Positive cycles, budget and batch lengths are required")
        collect(args)


if __name__ == "__main__":
    main()
