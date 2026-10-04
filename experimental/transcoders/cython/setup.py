"""Build the isolated experimental Cython kernel without production imports."""
from setuptools import setup, Extension
from Cython.Build import cythonize
setup(name='sqlaero-cython-kernel',version='0.0.1',ext_modules=cythonize([Extension('sqlaero_cython_kernel',['sqlaero_cython_kernel.pyx'])],compiler_directives={'language_level':3}))
