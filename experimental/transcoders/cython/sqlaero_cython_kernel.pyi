"""Public signatures for the independent experimental Cython kernel."""
from typing import Any
class EscapeError(Exception):
    """Base escaping failure; uses public sqlaero errors when installed."""
class EscapeTypeError(EscapeError, TypeError):
    """Unsupported input type."""
class EscapeValueError(EscapeError, ValueError):
    """Invalid input value."""
class DecodeTypeError(ValueError):
    """Unsupported decoding operation."""
class DecodeValueError(ValueError):
    """Malformed or overflowing field value."""
def configure_types(*, objstr: tuple[type, ...] = (), rawtext: tuple[type, ...] = (), sqlfunction: tuple[type, ...] = (), sqlinterval: tuple[type, ...] = ()) -> None:
    """Register trusted shared bridge and SQL expression types."""
def escape(data: object, many: bool = False, itemize: bool = True) -> str | tuple[str, ...] | list[str] | list[tuple[str, ...]]:
    """Return one SQL scalar literal or container parameter shape."""
def decode(value: bytes, field_type: int, encoding: bytes, is_binary: bool, use_decimal: bool = False, decode_bit: bool = False, decode_json: bool = False) -> Any:
    """Decode a MySQL text protocol value by its field code."""
def dumps(value: object) -> bytes:
    """Write compact strict UTF-8 JSON independently."""
def loads(value: bytes | str) -> Any:
    """Parse a complete strict JSON document independently."""
class _JSONCodec:
    """Convenient JSON namespace with the same independent implementations."""
    def dumps(self, value: object) -> bytes:
        """Serialize the supported JSON object domain."""
    def loads(self, value: bytes | str) -> Any:
        """Parse strict JSON bytes or Unicode."""
json: _JSONCodec
def dispatch_code(value: object) -> int:
    """Experimental primitive classification probe, without serialization or callbacks."""
