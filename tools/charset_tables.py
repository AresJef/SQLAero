"""Generate factual MySQL codec tables from the two private test instances.

This is a bounded build-time data extractor, not a product runtime codec.
It copies no server source code; every mapping is an observed SQL result.
"""

from __future__ import annotations

import hashlib
import json
import argparse
from collections.abc import Iterable
from pathlib import Path
import re

from mysql_fixture import mysql_command, run

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "evidence/rust-sync-kernel/charset"


def single_byte_names() -> list[str]:
    """Use the already frozen catalog's route order, never an extra type list."""
    text = (ROOT / "crates/sqlaero-core/src/charset/catalog.rs").read_text()
    entries = re.findall(r'name: "([a-z0-9]+)",\s*default_collation:[^}]+?SingleByte\((\d+)\)', text)
    ordered = sorted((int(index), name) for name, index in entries)
    if [index for index, _ in ordered] != list(range(24)):
        raise RuntimeError("single-byte route identity changed")
    return [name for _, name in ordered]


def expression(name: str, byte: int) -> tuple[str, str]:
    """Return trusted-catalog SQL expressions for strict observed conversion."""
    source = f"CONVERT(X'{byte:02X}' USING {name})"
    unicode = f"CONVERT({source} USING utf8mb4)"
    return unicode, f"CONVERT({unicode} USING {name})"


def collect(label: str, names: list[str]) -> dict[str, list[tuple[int, int] | None]]:
    """Collect each byte plus its canonical reverse, rejecting server warnings."""
    statements: list[str] = []
    for name in names:
        for byte in range(256):
            unicode, reverse = expression(name, byte)
            statements.append(f"SELECT 'M','{name}',{byte},HEX({unicode}),HEX({reverse}); SHOW WARNINGS;")
    raw = run(mysql_command(label), data="\n".join(statements))
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    (EVIDENCE / f"single-byte-mysql{label}.tsv").write_text(raw)
    tables: dict[str, list[tuple[int, int] | None]] = {name: [None] * 256 for name in names}
    active: tuple[str, int] | None = None
    seen: set[tuple[str, int]] = set()
    for line in raw.splitlines():
        fields = line.split("\t")
        if fields[0] == "M":
            _, name, byte_text, utf8, encoded = fields
            byte = int(byte_text)
            active = (name, byte)
            if active in seen:
                raise RuntimeError("duplicate conversion observation")
            seen.add(active)
            if utf8 != "NULL" and encoded != "NULL":
                text = bytes.fromhex(utf8).decode("utf-8", "strict")
                reverse = bytes.fromhex(encoded)
                if len(text) != 1 or len(reverse) != 1:
                    raise RuntimeError("single-byte conversion did not yield one scalar/byte")
                # Several legacy server tables use U+FFFD for unassigned slots
                # without emitting Warning 1300. Strict codecs must not turn
                # those slots into silently replaced data.
                if ord(text) != 0xFFFD and not (text == "?" and reverse[0] != byte):
                    tables[name][byte] = (ord(text), reverse[0])
        elif fields[0] in ("Warning", "Error"):
            if active is None:
                raise RuntimeError("warning before its conversion observation")
            # Deprecation describes the family, not the validity of this byte.
            # Preserve it in raw evidence without deleting accurate mappings.
            if fields[0] == "Warning" and fields[1] == "1287":
                continue
            if fields[0] == "Error" or fields[1] != "1300":
                raise RuntimeError("unexpected conversion diagnostic requires review")
            name, byte = active
            tables[name][byte] = None
        else:
            raise RuntimeError("unexpected conversion output")
    if len(seen) != len(names) * 256:
        raise RuntimeError("incomplete single-byte observation matrix")
    return tables


def emit(names: list[str], tables: dict[str, list[tuple[int, int] | None]]) -> None:
    """Emit one static decoder and canonical encoder for each catalog route."""
    lines = [
        "//! Observed MySQL 8.4.13/9.7.4 single-byte mappings; no server source copied.",
        "//! Reproduce with tools/charset_tables.py. Source output identities are",
        "//! recorded in evidence/rust-sync-kernel/charset/single-byte-identity.json.",
        "",
        "pub(super) static DECODE: [[u32; 256]; 24] = [",
    ]
    reverses: list[list[tuple[int, int]]] = []
    for name in names:
        table = tables[name]
        lines.append(f"    // {name}")
        lines.append("    [")
        for offset in range(0, 256, 16):
            lines.append("        " + ", ".join(f"0x{entry[0]:X}" if entry else "0xFFFFFFFF" for entry in table[offset:offset + 16]) + ",")
        lines.append("    ],")
        reverse: dict[int, int] = {}
        for entry in table:
            if entry is not None:
                point, byte = entry
                if point in reverse and reverse[point] != byte:
                    raise RuntimeError("inconsistent canonical reverse conversion")
                reverse[point] = byte
        reverses.append(sorted(reverse.items()))
    lines.append("];\n")
    lines.append("pub(super) static ASCII_IDENTITY: [bool; 24] = [")
    for name in names:
        identity = all(tables[name][byte] == (byte, byte) for byte in range(128))
        lines.append(f"    {str(identity).lower()}, // {name}")
    lines.append("];\n")
    lines.append("pub(super) static ENCODE: [&[(u32, u8)]; 24] = [")
    for name, entries in zip(names, reverses, strict=True):
        lines.append(f"    // {name}")
        lines.append("    &[")
        for offset in range(0, len(entries), 8):
            lines.append("        " + ", ".join(f"(0x{point:X}, 0x{byte:02X})" for point, byte in entries[offset:offset + 8]) + ",")
        lines.append("    ],")
    lines.append("];\n")
    output = ROOT / "crates/sqlaero-core/src/charset/single_byte.rs"
    output.write_text("\n".join(lines))
    identity = {
        "names": names, "observations_per_server": 24 * 256,
        "both_server_mappings_identical": True,
        "generated_sha256": hashlib.sha256(output.read_bytes()).hexdigest(),
        "raw_sha256": {label: hashlib.sha256((EVIDENCE / f"single-byte-mysql{label}.tsv").read_bytes()).hexdigest() for label in ("84", "97")},
        "valid_bytes": {name: sum(entry is not None for entry in tables[name]) for name in names},
    }
    (EVIDENCE / "single-byte-identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    print(json.dumps({"routes": len(names), "observations_per_server": 24 * 256, "both_identical": True}))


def main() -> None:
    """Require agreement from both supported server lines before saving tables."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--multibyte", action="store_true")
    parser.add_argument("--saved", action="store_true", help="recompile already saved immutable SQL observations")
    arguments = parser.parse_args()
    if arguments.multibyte:
        multibyte(saved=arguments.saved)
        return
    names = single_byte_names()
    first = collect("84", names)
    second = collect("97", names)
    if first != second:
        raise RuntimeError("supported MySQL lines disagree; no table emitted")
    emit(names, first)


def sequences(name: str) -> Iterable[bytes]:
    """Cover every one/two-byte candidate and the full applicable JIS plane."""
    for byte in range(256):
        yield bytes([byte])
    for first in range(128, 256):
        for second in range(256):
            yield bytes([first, second])
    if name in ("ujis", "eucjpms"):
        for second in range(0xA1, 0xFF):
            for third in range(0xA1, 0xFF):
                yield bytes([0x8F, second, third])
    if name == "gb18030":
        # Independent PSF GB18030 route supplies BMP four-byte candidates;
        # MySQL remains the value oracle. It is never a runtime fallback.
        extra: set[bytes] = set()
        for point in range(0x10000):
            if 0xD800 <= point <= 0xDFFF:
                continue
            encoded = chr(point).encode("gb18030")
            if len(encoded) == 4:
                extra.add(encoded)
        for plane in range(1, 17):
            for offset in (0, 1, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF):
                point = plane * 0x10000 + offset
                if point <= 0x10FFFF:
                    extra.add(chr(point).encode("gb18030"))
        # Invalid-range boundaries and valid upper endpoints from the public
        # MySQL GB18030-2005 contract are tested as bytes, not assumed valid.
        extra.update(bytes.fromhex(value) for value in ("8431A439", "8431A438", "90308130", "E3329A35", "E3329A36", "EF39EF39", "FE39FE39"))
        yield from sorted(extra)


def multibyte(*, saved: bool = False) -> None:
    """Observe complete bounded matrices, retaining forward diagnostics only."""
    names = ["big5", "cp932", "eucjpms", "euckr", "gb18030", "gb2312", "gbk", "sjis", "ujis"]
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    identity: dict[str, object] = {}
    for name in names:
        candidates = list(sequences(name))
        normalized: dict[str, list[tuple[str, str | None, str | None]]] = {}
        for label in ("84", "97"):
            statements: list[str] = []
            for encoded in candidates:
                source = f"CONVERT(X'{encoded.hex()}' USING {name})"
                unicode = f"CONVERT({source} USING utf8mb4)"
                reverse = f"CONVERT({unicode} USING {name})"
                statements.append(f"SELECT 'M','{encoded.hex()}',HEX({unicode}),HEX({reverse}); SHOW WARNINGS;")
            raw_path = EVIDENCE / f"{name}-mysql{label}.tsv"
            raw = raw_path.read_text() if saved else run(mysql_command(label), data="\n".join(statements))
            if not saved:
                raw_path.write_text(raw)
            entries: list[tuple[str, str | None, str | None]] = []
            for line in raw.splitlines():
                fields = line.split("\t")
                if fields[0] == "M":
                    _, code, unicode_hex, reverse_hex = fields
                    text = None if unicode_hex == "NULL" else bytes.fromhex(unicode_hex).decode("utf8", "strict")
                    valid = text is not None and len(text) == 1 and (text != "\ufffd" or name == "gb18030")
                    if text == "?" and code.lower() != "3f":
                        valid = False
                    entries.append((code, unicode_hex if valid else None, reverse_hex if valid and reverse_hex != "NULL" else None))
                elif fields[0] == "Warning" and fields[1] == "1287":
                    continue
                elif fields[0] == "Warning" and fields[1] in ("1300", "1366"):
                    code, _, _ = entries[-1]
                    entries[-1] = (code, None, None)
                else:
                    raise RuntimeError(f"unexpected {name} conversion diagnostic: {fields[:2]}")
            if len(entries) != len(candidates):
                raise RuntimeError("incomplete multibyte observation matrix")
            normalized[label] = entries
        if normalized["84"] != normalized["97"]:
            raise RuntimeError(f"supported MySQL lines disagree for {name}")
        output = EVIDENCE / f"{name}-observed.tsv"
        output.write_text("".join(f"{code}\t{point or '-'}\t{reverse or '-'}\n" for code, point, reverse in normalized["84"]))
        details: dict[str, object] = {"observations_per_server": len(candidates), "valid_single_characters": sum(point is not None for _, point, _ in normalized["84"]), "sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "both_identical": True}
        identity[name] = details
        print(json.dumps({"family": name, **details}), flush=True)
    (EVIDENCE / "multibyte-identity.json").write_text(json.dumps(identity, indent=2) + "\n")


if __name__ == "__main__":
    main()
