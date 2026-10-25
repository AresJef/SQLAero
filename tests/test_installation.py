"""Exercise the installed extension and its actual link to the native core."""

# This boundary test deliberately calls a private installation probe.
# pyright: reportPrivateUsage=false

import pytest

from sqlaero import _sqlaero


def test_installed_extension_calls_shared_native_size_parser() -> None:
    assert _sqlaero._parse_size("64MB") == 67_108_864
    assert _sqlaero._parse_size("16 KiB") == 16_384


def test_invalid_size_propagates_across_extension_boundary() -> None:
    with pytest.raises(ValueError, match="size unit"):
        _sqlaero._parse_size("1.5MB")
