"""Consumer inference is checked from calls, not manually annotated results."""
from typing import assert_type
from .retry_provider import get_count

assert_type(get_count(7), int)
assert_type(get_count(7, strict=False), int)
