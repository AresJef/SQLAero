"""Private native extension declarations."""

from typing import Any

class SQLAeroError(Exception):
    """Native root exception; the public errors module is not yet complete."""
    def __init__(self, message: str = '', *, code: int | None = None,
                 is_server_error: bool | None = None) -> None: ...
    @property
    def message(self) -> str: ...
    @property
    def code(self) -> int | None: ...
    @property
    def is_server_error(self) -> bool | None: ...

def _parse_size(value: str) -> int:
    """Parse a textual byte count using the shared native configuration parser."""
    ...
def _decode_json(data: bytes) -> Any:
    """Decode one complete JSON value with strict native numeric checks."""
    ...
