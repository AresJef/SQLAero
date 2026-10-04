"""Build an isolated ASan/UBSan extension and run only small owned/shared correctness tests."""
import os
import subprocess
import sysconfig
from pathlib import Path

def main() -> int:
    """Use existing clang/SDK/runtime with process-local flags and retain owned validation logs."""
    root=Path(__file__).resolve().parent
    out=Path('/tmp/sqlaero-cython-sanitized');out.mkdir(exist_ok=True)
    sdk=subprocess.check_output(['xcrun','--show-sdk-path'],text=True).strip()
    cmd=['/Library/Developer/CommandLineTools/usr/bin/clang','-isysroot',sdk,'-bundle','-undefined','dynamic_lookup','-fsanitize=address,undefined','-fno-omit-frame-pointer','-O1','-g','-I'+sysconfig.get_path('include'),str(root/'sqlaero_cython_kernel.c'),'-o',str(out/('sqlaero_cython_kernel'+sysconfig.get_config_var('EXT_SUFFIX')))]
    result=subprocess.run(cmd,capture_output=True,text=True)
    (root/'validation/sanitizer-build-final.log').write_text(' '.join(cmd)+'\n'+result.stdout+result.stderr)
    print('sanitizer build exit',result.returncode,flush=True)
    if result.returncode:return result.returncode
    runtime='/Library/Developer/CommandLineTools/usr/lib/clang/21/lib/darwin'
    env=os.environ.copy()
    env.update(DYLD_INSERT_LIBRARIES=runtime+'/libclang_rt.asan_osx_dynamic.dylib',DYLD_LIBRARY_PATH=runtime,ASAN_OPTIONS='detect_leaks=0:abort_on_error=1',UBSAN_OPTIONS='halt_on_error=1:print_stacktrace=1',SQLAERO_KERNEL='sqlaero_cython_kernel',PYTHONPATH=str(out)+':/tmp/sqlaero-transcoder-bridge-lib')
    script="""import sys,unittest
sys.path.insert(0,'/tmp/sqlaero-cython-sanitized')
import sqlaero_cython_kernel as k
assert k.__file__.startswith('/tmp/sqlaero-cython-sanitized/')
print('SANITIZED_MODULE',k.__file__,flush=True)
loader=unittest.TestLoader();suite=unittest.TestSuite()
suite.addTests(loader.discover('/Users/jef/ResilioSync/code/SQLAero/experimental/transcoders/cython',pattern='test_*.py'))
suite.addTests(loader.discover('/Users/jef/ResilioSync/code/SQLAero/experimental/transcoders/tests',pattern='test_*.py',top_level_dir='/Users/jef/ResilioSync/code/SQLAero/experimental/transcoders/tests'))
result=unittest.TextTestRunner(verbosity=1).run(suite)
raise SystemExit(0 if result.wasSuccessful() else 1)
"""
    result=subprocess.run(['/tmp/sqlaero-pypi-release-verification/bin/python','-B','-c',script],capture_output=True,text=True,env=env,timeout=60)
    (root/'validation/sanitizer-tests.log').write_text(result.stdout+result.stderr)
    print('sanitizer tests exit',result.returncode)
    print(result.stdout+result.stderr)
    return result.returncode

if __name__=='__main__':raise SystemExit(main())
