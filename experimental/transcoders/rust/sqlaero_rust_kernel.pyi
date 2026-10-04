"""Typed API for the independent experimental Rust transcoder."""
from typing import Any, Literal

def escape(data: object, many: bool = False, itemize: bool = True) -> str | tuple[str, ...] | list[str] | list[tuple[str, ...]]:
    """Escape values or produce ordered parameter items according to shape flags."""
    ...
def decode(value: bytes, field_type: int, encoding: bytes, is_binary: bool, use_decimal: bool = False, decode_bit: bool = False, decode_json: bool = False) -> object:
    """Decode a MySQL text-protocol value with checked numeric and date grammar."""
    ...
def dumps(value: object) -> bytes:
    """Encode the supported JSON domain with strict Unicode and bounded resources."""
    ...
def loads(value: bytes | str) -> object:
    """Parse a complete strict UTF-8 JSON document using the independent grammar."""
    ...
def configure_types(*, objstr: tuple[type, ...] = (), rawtext: tuple[type, ...] = (), sqlfunction: tuple[type, ...] = (), sqlinterval: tuple[type, ...] = ()) -> None:
    """Internal bridge integration: set reviewed SQL trust classes, replacing earlier tuples."""
    ...
def _set_dispatch_strategy(strategy: Literal['cascade', 'type_id', 'function_table', 'hot_registry']) -> None:
    """Internal research switch: select classifier feeding identical value handlers."""
    ...
class _JSON:
    """JSON namespace sharing the independent top-level encoder and parser."""
    @staticmethod
    def dumps(value: object) -> bytes:
        """Encode the supported JSON domain."""
        ...
    @staticmethod
    def loads(value: bytes | str) -> object:
        """Parse a complete strict JSON document."""
        ...
json: _JSON
