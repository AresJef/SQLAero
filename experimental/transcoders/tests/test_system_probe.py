"""Mock OS-reader tests; never collect machine load or run benchmarks automatically."""
import importlib.util
from pathlib import Path
import sys
import unittest

SOURCE = Path(__file__).resolve().parents[1] / 'benchmarking' / 'system_probe.py'
SPEC = importlib.util.spec_from_file_location('sqlaero_system_probe_tests', SOURCE)
assert SPEC is not None and SPEC.loader is not None
probe = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = probe
SPEC.loader.exec_module(probe)

class ProbeTests(unittest.TestCase):
    """Validate genuine source parsing, missing-signal policy and retained command evidence."""
    def reader(self, overrides: dict[tuple[str, ...], object] | None = None) -> probe.CommandRunner:
        """Create a bounded fake command reader retaining exact command provenance."""
        data = {
            probe.TOP_COMMAND: 'CPU usage: 10% user, 10% sys, 80% idle\nCPU usage: 2.5% user, 3.5% sys, 94% idle\n',
            probe.CPU_COUNT_COMMAND: '8\n',
            probe.PROCESS_STATE_COMMAND: 'R+\nS\nR\nU\nI\nZ\nT\n',
            probe.PRESSURE_COMMAND: '2\n',
        }
        data.update(overrides or {})
        def run(command: tuple[str, ...], timeout: float) -> probe.CommandResult:
            """Return specified OS output or a preserved simulated command failure."""
            value = data[command]
            if isinstance(value, Exception):
                raise value
            if isinstance(value, probe.CommandResult):
                return value
            return probe.CommandResult(command, 0, str(value), elapsed_seconds=0.1)
        return run

    def test_available_sources(self) -> None:
        """Use second CPU delta, exact logical denominator, process R states and raw pressure level."""
        obj = probe.MacOSSystemProbe(self.reader(), system='Darwin')
        report = obj.sample()
        self.assertEqual(report.signals, {'cpu_busy_percent': 6.0, 'runnable_per_cpu': .25, 'memory_pressure': 2.0})
        self.assertEqual(report.logical_cpus, 8)
        self.assertEqual(len(report.commands), 4)
        self.assertEqual(report.capabilities['memory_pressure']['status'], 'available')
        self.assertIs(obj.latest, report)
        self.assertEqual(report.as_dict()['normalization'], 'logical CPUs reported by hw.logicalcpu')

    def test_permission_and_timeout(self) -> None:
        """Keep unavailable failure provenance rather than replacing missing signals with guesses."""
        reader = self.reader({probe.TOP_COMMAND: probe.CommandResult(probe.TOP_COMMAND, 1, stderr='Operation not permitted'),
                              probe.PRESSURE_COMMAND: TimeoutError('no response')})
        report = probe.MacOSSystemProbe(reader, system='Darwin').sample()
        self.assertIsNone(report.signals['cpu_busy_percent'])
        self.assertIsNone(report.signals['memory_pressure'])
        self.assertEqual(report.signals['runnable_per_cpu'], .25)
        self.assertIn('not permitted', report.capabilities['cpu_busy_percent']['reason'])
        self.assertIn('TimeoutError', report.capabilities['memory_pressure']['reason'])

    def test_no_since_start_cpu_fallback(self) -> None:
        """Reject an initial-only sample and inconsistent percentages."""
        for text in ('CPU usage: 1% user, 2% sys, 97% idle\n',
                     'CPU usage: 1% user, 2% sys, 97% idle\nCPU usage: 60% user, 60% sys, 0% idle\n',
                     'CPU usage: 1% user, 2% sys, 97% idle\nCPU usage: 1% user, 2% sys, 97% idle\nCPU usage: unavailable\n'):
            report = probe.MacOSSystemProbe(self.reader({probe.TOP_COMMAND: text}), system='Darwin').sample()
            self.assertIsNone(report.signals['cpu_busy_percent'])

    def test_runnable_requires_real_denominator_and_valid_rows(self) -> None:
        """Never guess CPU counts or turn malformed/empty process snapshots into zero load."""
        for overrides in ({probe.CPU_COUNT_COMMAND: '0'}, {probe.CPU_COUNT_COMMAND: 'unknown'},
                          {probe.PROCESS_STATE_COMMAND: ''}, {probe.PROCESS_STATE_COMMAND: 'STATE\nR\n'}):
            report = probe.MacOSSystemProbe(self.reader(overrides), system='Darwin').sample()
            self.assertIsNone(report.signals['runnable_per_cpu'])
        report = probe.MacOSSystemProbe(self.reader({probe.PROCESS_STATE_COMMAND: 'S\nI\n'}), system='Darwin').sample()
        self.assertEqual(report.signals['runnable_per_cpu'], 0)

    def test_memory_is_not_free_percent_or_paging(self) -> None:
        """Reject alternative representations instead of silently weakening pressure policy."""
        for text in ('System-wide memory free percentage: 80%', 'Swapouts: 0.', '-1', 'nan'):
            report = probe.MacOSSystemProbe(self.reader({probe.PRESSURE_COMMAND: text}), system='Darwin').sample()
            self.assertIsNone(report.signals['memory_pressure'])

    def test_unsupported_platform_and_bad_provenance(self) -> None:
        """Unsupported OS runs no commands, and mismatched injected command records fail closed."""
        def fail(command: tuple[str, ...], timeout: float) -> probe.CommandResult:
            """Prove unsupported-platform samples do not execute the injected runner."""
            raise AssertionError('must not run')
        report = probe.MacOSSystemProbe(fail, system='Linux').sample()
        self.assertEqual(report.commands, ())
        self.assertTrue(all(value is None for value in report.signals.values()))
        bad = probe.CommandResult(('different',), 0, '1')
        report = probe.MacOSSystemProbe(self.reader({probe.PRESSURE_COMMAND: bad}), system='Darwin').sample()
        self.assertIsNone(report.signals['memory_pressure'])

    def test_callable_and_timeout_validation(self) -> None:
        """Expose only protocol signals and reject impossible timeout configurations."""
        obj = probe.MacOSSystemProbe(self.reader(), system='Darwin')
        self.assertEqual(set(obj()), set(probe.SIGNAL_NAMES))
        self.assertIsNotNone(obj.latest)
        for timeout in (0, 1, float('nan'), float('inf')):
            with self.assertRaises(ValueError): probe.MacOSSystemProbe(timeout_seconds=timeout)

if __name__ == '__main__': unittest.main()
