"""Exercise the installed extension and its actual link to the native core."""

# This boundary test deliberately calls a private installation probe.
# pyright: reportPrivateUsage=false

from collections.abc import Iterator
import sys
import sysconfig

import pytest

from sqlaero import _sqlaero


@pytest.fixture(scope='session', autouse=True)
def retain_real_free_threaded_mode() -> Iterator[None]:
    """Do not let imports or later native work turn NoGIL evidence into GIL tests."""
    free_threaded = bool(sysconfig.get_config_var('Py_GIL_DISABLED'))
    enabled = getattr(sys, '_is_gil_enabled', None)
    if free_threaded:
        assert enabled is not None and not enabled(), 'The t installation enabled the GIL'
    yield
    if free_threaded:
        assert enabled is not None and not enabled(), 'The integration session enabled the GIL'


def test_installed_extension_calls_shared_native_size_parser() -> None:
    assert _sqlaero._parse_size("64MB") == 67_108_864
    assert _sqlaero._parse_size("16 KiB") == 16_384


def test_invalid_size_propagates_across_extension_boundary() -> None:
    with pytest.raises(ValueError, match="size unit"):
        _sqlaero._parse_size("1.5MB")
