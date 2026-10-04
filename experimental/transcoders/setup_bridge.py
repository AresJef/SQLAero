"""Build the isolated experimental Cython class and ABI bridge."""
from pathlib import Path
from setuptools import Extension, setup
from Cython.Build import cythonize
root = Path(__file__).resolve().parent
setup(name="sqlaero-experimental-bridge", version="0.0.0",
      ext_modules=cythonize([Extension("sqlaero_transcoder_bridge",
          [str(root / "shared" / "sqlaero_transcoder_bridge.pyx")])],
          compiler_directives={"language_level": 3}))
