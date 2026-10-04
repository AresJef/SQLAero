"""Build only the newly compiled Cython consumer compatibility probe."""
from pathlib import Path
from setuptools import Extension,setup
from Cython.Build import cythonize
root=Path(__file__).resolve().parent
setup(name='sqlaero-transcoder-probe',version='0.0.0',ext_modules=cythonize(
    [Extension('sqlaero_abi_probe',[str(root/'tests'/'abi_probe.pyx')])],
    include_path=[str(root/'shared')],compiler_directives={'language_level':3}))
