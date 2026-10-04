"""Read-only environment observations and a cooperative host benchmark lease.

Copy this standalone module unchanged to another project. All participants must
use the same lock path; the lease cannot exclude nonparticipating processes.
Missing signals remain missing. No privilege escalation or system changes.
"""
from dataclasses import asdict, dataclass
import datetime as dt
import getpass
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import threading
import time

PROTOCOL = "host-aware-paired-v2"


@dataclass(frozen=True)
class BenchmarkPolicy:
    load_fraction: float = .45
    cpu_busy_fraction: float = .45
    minimum_available_memory_fraction: float = .05
    stable_seconds: float = 30
    poll_seconds: float = 2
    maximum_wait_seconds: float = 600
    auxiliary_probe_seconds: float = 10

    def __post_init__(self):
        for name in ("load_fraction", "cpu_busy_fraction", "minimum_available_memory_fraction"):
            value = getattr(self, name)
            if not math.isfinite(value) or not 0 < value <= 1:
                raise ValueError(name)
        for name in ("poll_seconds", "maximum_wait_seconds", "auxiliary_probe_seconds"):
            value = getattr(self, name)
            if not math.isfinite(value) or value <= 0:
                raise ValueError(name)
        if not math.isfinite(self.stable_seconds) or self.stable_seconds < 0:
            raise ValueError("stable_seconds")


def shared_lock_path():
    configured = os.environ.get("BENCHMARK_SHARED_LOCK_PATH")
    if configured:
        return Path(configured).expanduser().absolute()
    # Same user and host, independent of the repository and Python interpreter.
    identity = str(os.getuid()) if hasattr(os, "getuid") else getpass.getuser()
    token = hashlib.sha256(identity.encode()).hexdigest()[:16]
    return Path(tempfile.gettempdir()) / ("shared-benchmark-" + token + ".lock")


class BenchmarkLease:
    """Kernel-held advisory lock; stale metadata never authorizes unlink/kill."""
    def __init__(self, path=None):
        self.path = Path(path) if path is not None else shared_lock_path()
        self.handle = None

    def acquire(self, deadline, *, owner, poll_seconds=.2):
        if self.handle is not None:
            raise RuntimeError("Lease already held")
        self.path.parent.mkdir(parents=True, exist_ok=True)
        descriptor = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
        handle = os.fdopen(descriptor, "r+b")
        # Windows locks one byte; retain that byte on all platforms.
        handle.seek(0, os.SEEK_END)
        if not handle.tell():
            handle.write(b"\0"); handle.flush()
        while True:
            try:
                if os.name == "nt":
                    import msvcrt
                    handle.seek(0); msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except (BlockingIOError, OSError) as exc:
                import errno
                if exc.errno not in (errno.EAGAIN, errno.EACCES, errno.EDEADLK):
                    handle.close(); raise
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    handle.close(); raise TimeoutError("Shared benchmark lease wait exhausted")
                time.sleep(min(poll_seconds, remaining))
        self.handle = handle
        metadata = dict(protocol=PROTOCOL, pid=os.getpid(), owner=owner,
                        acquired_at=dt.datetime.now(dt.timezone.utc).isoformat())
        handle.seek(1); handle.write(json.dumps(metadata).encode()); handle.truncate(); handle.flush()
        return dict(path=str(self.path.absolute()), **metadata)

    def release(self):
        if self.handle is not None:
            handle, self.handle = self.handle, None
            try:
                if os.name == "nt":
                    import msvcrt
                    handle.seek(0); msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
            finally:
                handle.close()

    def __enter__(self):
        if self.handle is None:
            raise RuntimeError("Acquire the lease before entering")
        return self

    def __exit__(self, *unused):
        self.release()


def unavailable(reason):
    return dict(status="unavailable", reason=reason)


class EnvironmentObserver:
    def __init__(self, policy=None):
        self.policy = policy or BenchmarkPolicy()
        try:
            import psutil
            self.psutil = psutil
        except ImportError:
            self.psutil = None
        self.previous_cpu = None
        self.previous_pressure = None
        self.auxiliary_at = -math.inf
        self.auxiliary = {}

    def _command(self, command):
        try:
            result = subprocess.run(command, capture_output=True, text=True, timeout=2)
            return dict(command=command, returncode=result.returncode,
                        stdout=result.stdout, stderr=result.stderr)
        except (OSError, subprocess.TimeoutExpired) as exc:
            return dict(command=command, error=type(exc).__name__ + ": " + str(exc))

    def _auxiliary(self, now):
        if now - self.auxiliary_at < self.policy.auxiliary_probe_seconds:
            return self.auxiliary
        self.auxiliary_at = now
        result = dict(temperature=unavailable("No supported unprivileged temperature reader"),
                      throttle=unavailable("No supported unprivileged throttle reader"),
                      memory_pressure=unavailable("No supported pressure reader"))
        if platform.system() == "Darwin":
            vm = self._command(["vm_stat"])
            counters = {name: int(value) for name, value in
                        re.findall(r"^(Pageouts|Swapins|Swapouts):\s+(\d+)\.", vm.get("stdout", ""), re.M)}
            result["memory_pressure"] = (dict(status="available", source="vm_stat",
                counters=counters, raw=vm) if len(counters) == 3 else unavailable(json.dumps(vm)))
            thermal = self._command(["pmset", "-g", "therm"])
            limits = {name: int(value) for name, value in re.findall(
                r"(CPU_Speed_Limit|CPU_Scheduler_Limit)\s*=\s*(\d+)", thermal.get("stdout", ""))}
            result["throttle"] = (dict(status="available", limits=limits, raw=thermal)
                                  if limits else unavailable(json.dumps(thermal)))
        elif platform.system() == "Linux":
            # These Linux system paths are read-only platform interfaces.
            pressure = {}
            for resource in ("cpu", "memory", "io"):
                path = Path("/proc/pressure") / resource
                try:
                    pressure[resource] = dict(status="available", source=str(path), raw=path.read_text())
                except OSError as exc:
                    pressure[resource] = unavailable(str(exc))
            result["memory_pressure"] = dict(status="available" if pressure["memory"]["status"] == "available" else "unavailable", psi=pressure)
            if pressure["memory"]["status"] == "available":
                result["memory_pressure"]["counters"] = {
                    "memory_" + kind + "_stall_us": int(value) for kind, value in re.findall(
                        r"^(some|full).*?\btotal=(\d+)", pressure["memory"]["raw"], re.M)}
            if self.psutil is not None and hasattr(self.psutil, "sensors_temperatures"):
                try:
                    readings = self.psutil.sensors_temperatures()
                    if readings:
                        result["temperature"] = dict(status="available", source="psutil", readings={
                            name: [row._asdict() for row in rows] for name, rows in readings.items()})
                except (OSError, NotImplementedError) as exc:
                    result["temperature"] = unavailable(str(exc))
        self.auxiliary = result
        return result

    def snapshot(self):
        started = time.monotonic()
        result = dict(at=dt.datetime.now(dt.timezone.utc).isoformat(), monotonic=started,
                      logical_cpus=os.cpu_count(), platform=platform.system())
        try:
            result["aggregate_load"] = list(os.getloadavg())
        except (AttributeError, OSError):
            result["aggregate_load"] = None
        cpu = memory = unavailable("Optional psutil not installed")
        if self.psutil is not None:
            try:
                times = self.psutil.cpu_times()._asdict()
                total = sum(value for name, value in times.items() if name not in ("guest", "guest_nice"))
                idle = times.get("idle", 0) + times.get("iowait", 0)
                current = total, idle
                if self.previous_cpu is not None and total > self.previous_cpu[0]:
                    busy = 1 - (idle - self.previous_cpu[1]) / (total - self.previous_cpu[0])
                    cpu = dict(status="available", busy_fraction=max(0, min(1, busy)), source="psutil.cpu_times delta")
                else:
                    cpu = unavailable("CPU delta requires a preceding observation")
                self.previous_cpu = current
            except (OSError, NotImplementedError) as exc:
                cpu = unavailable(type(exc).__name__ + ": " + str(exc))
            try:
                values = self.psutil.virtual_memory()
                memory = dict(status="available", available_bytes=values.available,
                              total_bytes=values.total, available_fraction=values.available / values.total,
                              source="psutil.virtual_memory.available")
            except (OSError, NotImplementedError) as exc:
                memory = unavailable(type(exc).__name__ + ": " + str(exc))
        result.update(cpu=cpu, memory=memory, **self._auxiliary(started))
        result["auxiliary_observation_age_seconds"] = started - self.auxiliary_at
        counters = result["memory_pressure"].get("counters")
        if counters is not None:
            if self.previous_pressure is not None:
                result["paging_activity"] = {name: max(0, value - self.previous_pressure[name]) for name, value in counters.items()}
            self.previous_pressure = dict(counters)
        result["probe_elapsed_seconds"] = time.monotonic() - started
        return result


def environment_flags(snapshot, policy):
    """Return observed interference separately from unavailable observations."""
    flags, missing = [], []
    load, cpus = snapshot.get("aggregate_load"), snapshot.get("logical_cpus")
    observed_cpu = bool(load is not None and cpus)
    if observed_cpu and load[0] > cpus * policy.load_fraction:
        flags.append("load_headroom_exceeded")
    cpu = snapshot.get("cpu", {})
    if cpu.get("status") == "available":
        observed_cpu = True
        if cpu["busy_fraction"] > policy.cpu_busy_fraction:
            flags.append("cpu_busy_headroom_exceeded")
    else:
        missing.append("cpu_busy")
    if not observed_cpu:
        flags.append("no_cpu_gate_signal")
    memory = snapshot.get("memory", {})
    if memory.get("status") == "available":
        if memory["available_fraction"] < policy.minimum_available_memory_fraction:
            flags.append("available_memory_floor_exceeded")
    else:
        missing.append("available_memory")
    if any(snapshot.get("paging_activity", {}).values()):
        flags.append("paging_activity_observed")
    throttle = snapshot.get("throttle", {})
    if throttle.get("status") == "available":
        if any(value < 100 for value in throttle.get("limits", {}).values()):
            flags.append("cpu_limit_observed")
    else:
        missing.append("throttle")
    for signal in ("temperature", "memory_pressure"):
        if snapshot.get(signal, {}).get("status") != "available":
            missing.append(signal)
    for rows in snapshot.get("temperature", {}).get("readings", {}).values():
        if any(row.get("high") is not None and row["current"] >= row["high"] for row in rows):
            flags.append("reported_temperature_high_limit_observed")
            break
    return dict(flags=flags, unavailable_signals=missing,
                acceptable_for_start=not flags, exclusive_idle_host=False)


def wait_for_stable_window(observer, deadline, *, record=None, clock=time.monotonic,
                           sleep=time.sleep):
    """One bounded budget includes lock waiting and stability waiting.

    The first acceptable observation starts a window, rather than claiming that
    the preceding period was stable. Long observation gaps reset the window.
    """
    policy = observer.policy
    stable_since = previous = None
    observations = []
    while True:
        now = clock()
        if now > deadline:
            return dict(status="wait-budget-exhausted", observations=observations)
        snapshot = observer.snapshot()
        evaluated = environment_flags(snapshot, policy)
        entry = dict(snapshot=snapshot, evaluation=evaluated)
        observations.append(entry)
        if record is not None:
            record(entry)
        if clock() > deadline:
            return dict(status="wait-budget-exhausted", observations=observations)
        if not evaluated["acceptable_for_start"]:
            stable_since = None
        elif stable_since is None or (previous is not None and now - previous > policy.poll_seconds * 2):
            stable_since = now
        previous = now
        if stable_since is not None and now - stable_since >= policy.stable_seconds:
            return dict(status="stable-window-observed", observations=observations,
                        stable_elapsed_seconds=now - stable_since)
        remaining = deadline - clock()
        if remaining <= 0:
            return dict(status="wait-budget-exhausted", observations=observations)
        sleep(min(policy.poll_seconds, remaining))


class EnvironmentMonitor:
    """Sample around and during one child; interference never deletes its output."""
    def __init__(self, observer):
        self.observer = observer
        self.observations = []
        self.errors = []
        self.stop = threading.Event()
        self.thread = None

    def _observe(self, position):
        try:
            snapshot = self.observer.snapshot()
            self.observations.append(dict(position=position, snapshot=snapshot,
                evaluation=environment_flags(snapshot, self.observer.policy)))
        except Exception as exc:
            # Monitor failure is recorded, never silently treated as stability.
            self.errors.append(dict(position=position, error=type(exc).__name__ + ": " + str(exc)))

    def _during(self):
        while not self.stop.wait(self.observer.policy.poll_seconds):
            self._observe("during")

    def __enter__(self):
        self._observe("before")
        self.thread = threading.Thread(target=self._during, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *unused):
        self.stop.set()
        self.thread.join()
        self._observe("after")

    def result(self):
        flags = sorted({flag for row in self.observations for flag in row["evaluation"]["flags"]})
        return dict(protocol=PROTOCOL, observations=self.observations, monitor_errors=self.errors,
                    interference_flags=flags, flagged=bool(flags or self.errors),
                    samples_retained=True, probe_cost_included_in_shared_host_environment=True,
                    continuous_monitoring=False,
                    observation_interval_seconds=self.observer.policy.poll_seconds,
                    unavailable_signals_do_not_prove_normal_conditions=True)


def protocol_identity(policy):
    source = Path(__file__)
    return dict(protocol=PROTOCOL, policy=asdict(policy),
                module_sha256=hashlib.sha256(source.read_bytes()).hexdigest())


def verify_managed_receipt(path):
    """Check the parent controller's live lease before a managed child can time."""
    receipt = json.loads(Path(path).read_text())
    if receipt["protocol"] != PROTOCOL or receipt["controller_pid"] != os.getppid():
        raise ValueError("Managed benchmark parent/protocol mismatch")
    policy = BenchmarkPolicy(**receipt["policy_identity"]["policy"])
    if receipt["policy_identity"] != protocol_identity(policy):
        raise ValueError("Managed benchmark environment code/policy changed")
    lease_path = Path(receipt["lease"]["path"])
    metadata = json.loads(lease_path.read_bytes()[1:])
    if metadata["pid"] != receipt["controller_pid"] or metadata["protocol"] != PROTOCOL:
        raise ValueError("Managed benchmark lease owner mismatch")
    # Check kernel ownership without overwriting even stale owner metadata.
    with lease_path.open("r+b") as handle:
        try:
            if os.name == "nt":
                import msvcrt
                handle.seek(0); msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
        except (BlockingIOError, OSError) as exc:
            import errno
            if exc.errno in (errno.EAGAIN, errno.EACCES, errno.EDEADLK):
                return receipt
            raise
    raise ValueError("Managed benchmark lease is not held")
