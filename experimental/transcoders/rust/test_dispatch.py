"""Run the complete owned correctness suite independently for every classifier."""
import unittest
import sqlaero_rust_kernel as kernel
from test_kernel import KernelDeep

if __name__ == "__main__":
    success = True
    try:
        for strategy in kernel._dispatch_strategies():
            kernel._set_dispatch_strategy(strategy)
            # The selector test resets the strategy, so exclude it from each fixed run.
            names = [name for name in unittest.defaultTestLoader.getTestCaseNames(KernelDeep)
                     if name != "test_dispatch_candidates"]
            print("Classifier:", strategy, flush=True)
            suite = unittest.TestSuite(KernelDeep(name) for name in names)
            success &= unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful()
    finally:
        kernel._set_dispatch_strategy("hot_registry")
        kernel.configure_types()
    raise SystemExit(0 if success else 1)
