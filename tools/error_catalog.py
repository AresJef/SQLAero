"""Maintain SQLAero's single error-code registry and reproducible public outputs.

Inputs are frozen official installed message definitions, their matching C
headers, actual dedicated-server registration, and official global-error facts.
Only functional names/numbers/SQLSTATEs are extracted; Oracle message templates
and log-only MY codes are not copied into SQLAero's runtime or documentation.
"""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
from html import unescape
import json
from pathlib import Path
import re
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / 'evidence/rust-sync-kernel/startup'
REGISTRY = ROOT / 'data/error-catalog.json'
CLASSES = ('InterfaceError','InterfaceBusyError','DatabaseError','DataError','OperationalError',
           'OperationalTimeoutError','IntegrityError','InternalError','ProgrammingError','NotSupportedError')


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def message_fingerprints(source: str) -> dict[str, str]:
    """Fingerprint official English conditions without redistributing templates."""
    messages: dict[str, list[str]] = {}
    symbol: str | None = None
    continuing: str | None = None
    for line in source.splitlines():
        if entry := re.match(r'^((?:ER|WARN|OBSOLETE)_\w+)', line):
            symbol = entry[1]
            continuing = None
        elif symbol and (english := re.match(r'^\s+eng\s+(.+)$', line)):
            if symbol in messages:
                raise ValueError(f'Duplicate official English condition: {symbol}')
            messages[symbol] = [english[1]]
            continuing = symbol
        elif continuing is not None and line.lstrip().startswith('"'):
            messages[continuing].append(line.strip())
        else:
            continuing = None
    return {name: hashlib.sha256('\n'.join(parts).encode()).hexdigest()
            for name, parts in messages.items()}


def installed(version: str, label: str) -> tuple[dict[int, dict[str, str]], list[dict[str, str]], list[dict[str, Any]]]:
    source = Path(f'/opt/homebrew/opt/mysql@{version}/share/mysql/messages_to_clients.txt')
    header = Path(f'/opt/homebrew/opt/mysql@{version}/include/mysql/mysqld_error.h')
    observed_path = EVIDENCE / f'error-catalog-{label}-observed.json'
    observed: dict[int, tuple[str, str]] = {
        row[0]: (row[1], row[2]) for row in json.loads(observed_path.read_text(encoding='utf-8')) if row[0] is not None
    }
    definitions = {name: int(code) for name, code in re.findall(r'^#define (\w+) (\d+)$', header.read_text(encoding='utf-8'), re.M)}
    codes: dict[int, dict[str, str]] = {}
    excluded: list[dict[str, Any]] = []
    source_text = source.read_text(encoding='utf-8')
    fingerprints = message_fingerprints(source_text)
    number = 1000
    for line in source_text.splitlines():
        if start := re.match(r'^start-error-number\s+(\d+)', line):
            number = int(start[1])
        elif entry := re.match(r'^((?:ER|WARN|OBSOLETE)_\w+)(?:\s+(\w{5}))?', line):
            symbol, state = entry[1], entry[2] or 'HY000'
            if symbol.startswith('OBSOLETE_'):
                excluded.append({'version':version,'code':number,'name':symbol,'reason':'official obsolete entry'})
            elif number not in observed:
                if symbol not in ('ER_NO','ER_YES'):
                    raise ValueError(f'Unexpected unregistered condition: {symbol}')
                excluded.append({'version':version,'code':number,'name':symbol,'reason':'message fragment/Note helper, not an independently registered condition'})
            else:
                if definitions.get(symbol) != number or observed[number] != (symbol,state):
                    raise ValueError(f'Official header/source/runtime mismatch at {version} {number}: {symbol}')
                codes[number] = {'name':symbol,'sqlstate':state, 'message_sha256':fingerprints[symbol]}
            number += 1
    sources = [{'kind':kind,'path':str(path),'sha256':digest(path)} for kind,path in (
        ('official installed client messages',source),('official installed generated header',header),('actual dedicated-server registration',observed_path))]
    return codes, sources, excluded


def globals_for(label: str) -> tuple[dict[int, dict[str, str]], dict[str, str]]:
    path = EVIDENCE / f'global-errors-{label}.html'
    values: dict[int, dict[str, str]] = {}
    for block in re.findall(r'<li class="listitem">(.*?)</li>', path.read_text(encoding='utf-8'), re.S):
        text = unescape(re.sub(r'<[^>]*>', ' ', block))
        identity = re.search(r'Error number:\s*(\d+)\s*;\s*Symbol:\s*([A-Z][A-Z_0-9]+)', text)
        if identity is None:
            continue
        message = re.search(r'<p>\s*Message:(.*?)</p>', block, re.S)
        if message is None:
            raise ValueError(f'Official global condition has no message: {identity[1]}')
        condition = ' '.join(unescape(re.sub(r'<[^>]*>', ' ', message[1])).split())
        code = int(identity[1])
        if code in values:
            raise ValueError(f'Duplicate official global condition: {code}')
        values[code] = {'name':identity[2], 'sqlstate':'HY000',
                        'message_sha256':hashlib.sha256(condition.encode()).hexdigest()}
    if not values or any(code < 1 or code > 999 for code in values):
        raise ValueError('Invalid official global-error facts')
    version = '8.4' if label == '84' else '9.7'
    return values, {'kind':'official shared global error reference',
                    'url':f'https://dev.mysql.com/doc/mysql-errors/{version}/en/global-error-reference.html',
                    'path':str(path),'sha256':digest(path)}


def snapshot() -> None:
    """Refresh facts only; changed conditions require an explicit semantic review.

    Classes, explanations, client definitions and aliases are maintained in the
    declaration, never inferred from SQLSTATE, names or already generated Rust.
    All comparisons finish before atomic replacement of the previous registry.
    """
    maintained: dict[str, Any] = json.loads(REGISTRY.read_text(encoding='utf-8'))
    versions: dict[str, dict[int, dict[str, str]]] = {}
    sources: list[dict[str, str]] = []
    excluded: list[dict[str, Any]] = []
    for version, label in [('8.4', '84'), ('9.7', '97')]:
        codes, provenance, omitted = installed(version, label)
        shared, source = globals_for(label)
        versions[version] = codes | shared
        sources.extend(provenance + ([source] if source else []))
        excluded.extend(omitted)
    expected = {row['code']: row for row in maintained['server']}
    actual: dict[int, dict[str, dict[str, str]]] = {}
    for version, entries in versions.items():
        for code, facts in entries.items():
            actual.setdefault(code, {})[version] = facts
    changed = sorted(code for code in expected.keys() | actual.keys()
                     if code not in expected or code not in actual
                     or expected[code]['versions'] != actual[code])
    if changed:
        raise ValueError(f'Semantic review required for added, removed or changed conditions: {changed}')
    for origin in ('client', 'server'):
        for row in maintained[origin]:
            if row['class'] not in CLASSES or not row['meaning']:
                raise ValueError(f'Invalid maintained declaration: {origin} {row["code"]}')
    maintained.update(recorded_at=datetime.now(timezone.utc).isoformat(),
                      sources=sources, excluded=excluded)
    import os
    import tempfile
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=REGISTRY.parent,
                                         prefix='.error-catalog-', delete=False) as output:
            temporary = Path(output.name)
            output.write(json.dumps(maintained, ensure_ascii=False, indent=2) + '\n')
        os.replace(temporary, REGISTRY)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print(json.dumps({'server_union':len(expected),
                      'per_version':{version:len(entries) for version, entries in versions.items()},
                      'classes':dict(Counter(row['class'] for row in maintained['server']))}, sort_keys=True))


def quoted(value: str) -> str:
    return json.dumps(value,ensure_ascii=False)


def description(row: dict[str, Any], server: bool) -> str:
    if not server:
        return f"{row['meaning'].capitalize()} Code {row['code']}; client origin (is_server_error=False); exact exception class {row['class']}."
    variants = '; '.join(f"MySQL {version}: {entry['name']} ({entry['sqlstate']})" for version,entry in row['versions'].items())
    return f"Server condition: {row['meaning']}. Code {row['code']}; direct server ERR origin (is_server_error=True); exact exception class {row['class']}. {variants}. This mapping applies only to ERR, not ordinary Warning/Note responses."


def generate(check: bool) -> None:
    data: dict[str, Any] = json.loads(REGISTRY.read_text(encoding='utf-8'))
    checksum = digest(REGISTRY)
    client: list[dict[str, Any]] = data['client']
    server: list[dict[str, Any]] = data['server']
    banner = f'Generated from data/error-catalog.json (SHA-256 {checksum}); edit the registry and run tools/error_catalog.py.'
    rust = [f'//! {banner}', '', '/// Exact native exception categories used by the maintained error registry.',
            '#[derive(Debug, Clone, Copy, PartialEq, Eq)]','pub enum ExceptionClass {']
    for category in CLASSES:
        rust.extend([f'    /// The public {category} category.',f'    {category[:-5]},'])
    rust.extend(['}','', '/// Registered client codes outside MySQL unsigned 16-bit ERR codes.',
                 '#[derive(Debug, Clone, Copy, PartialEq, Eq)]','#[repr(u32)]','pub enum ClientErrorCode {'])
    for row in client:
        rust.extend([f"    /// {row['class']}: {row['meaning']}",f"    {row['rust_name']} = {row['code']},"])
    rust.extend(['}','', '/// One maintained client identity and its precise public behavior.', 'pub struct ClientEntry {',
                 '    /// Stable code and native identity.','    pub code: ClientErrorCode,',
                 '    /// Public IntEnum member name.','    pub name: &\'static str,',
                 '    /// Exact exception category.','    pub class: ExceptionClass,',
                 '    /// Complete English member documentation.','    pub doc: &\'static str,','}',
                 '/// All registered client errors in stable numeric order.','pub static CLIENT_CODES: &[ClientEntry] = &['])
    for row in client:
        rust.append(f"    ClientEntry {{ code: ClientErrorCode::{row['rust_name']}, name: {quoted(row['name'])}, class: ExceptionClass::{row['class'][:-5]}, doc: {quoted(description(row,False))} }},")
    rust.extend(['];','impl ClientErrorCode {','    /// The one registered source for this client code\'s class and public name.',
                 '    #[must_use]','    pub fn entry(self) -> &\'static ClientEntry {','        match self {'])
    rust.extend(f"            Self::{row['rust_name']} => &CLIENT_CODES[{index}]," for index,row in enumerate(client))
    rust.extend(['        }','    }','}','','pub mod catalog;','', '#[cfg(test)]', 'mod tests;', ''])
    table = [f'//! {banner}', 'use super::ExceptionClass;','',
             '/// One maintained server condition, with aliases and exact category.',
             'pub struct ServerEntry {','    /// Actual unsigned 16-bit protocol number.','    pub code: u16,',
             '    /// Current canonical symbol followed by any supported-line alias.','    pub names: &\'static [&\'static str],',
             '    /// Exact known-condition exception category.','    pub class: ExceptionClass,',
             '    /// Complete English documentation for this value and its aliases.','    pub doc: &\'static str,','}',
             '/// All maintained target conditions, sorted by actual numeric code.','pub static SERVER_CODES: &[ServerEntry] = &[']
    for row in server:
        names=list(dict.fromkeys([row['name'],*[value['name'] for value in row['versions'].values()]]))
        table.append(f"    ServerEntry {{ code: {row['code']}, names: &[{', '.join(quoted(name) for name in names)}], class: ExceptionClass::{row['class'][:-5]}, doc: {quoted(description(row,True))} }},")
    table.extend(['];','', '/// Match a known target code; only unknown codes use the confirmed SQLSTATE.',
                  '/// No message parsing, numeric-range guessing, or success-path lookup occurs.',
                  '#[must_use]','pub fn classify(code: u16, state: Option<[u8; 5]>) -> ExceptionClass {',
                  '    if let Ok(index) = SERVER_CODES.binary_search_by_key(&code, |entry| entry.code) { return SERVER_CODES[index].class; }',
                  '    match state.map(|state| [state[0],state[1]]) {',
                  "        Some([b'0',b'8']) => ExceptionClass::Operational,",
                  "        Some([b'0',b'A']) => ExceptionClass::NotSupported,",
                  "        Some([b'2',b'2']) => ExceptionClass::Data,",
                  "        Some([b'2',b'3']) => ExceptionClass::Integrity,",
                  "        Some([b'4',b'2']) => ExceptionClass::Programming,",
                  '        _ => ExceptionClass::Database,','    }','}',''])
    declarations = ['"""'+banner+'"""','from enum import IntEnum', 'from ._sqlaero import (']
    declarations.extend(f'    {name} as {name},' for name in ('SQLAeroError', *CLASSES, 'SQLAeroWarning', 'ErrorInfo'))
    declarations.extend([')', '', 'class SQLAeroErrorCode(IntEnum):',
                    '    """Stable SQLAero client-origin codes; each member documents its exact class."""'])
    for row in client:
        declarations.extend([f"    {row['name']} = {row['code']}", f'    {quoted(description(row,False))}'])
    declarations.extend(['','class MySQLErrorCode(IntEnum):','    """Known direct server ERR numbers; ordinary warnings are not automatically raised."""'])
    for row in server:
        names=list(dict.fromkeys([row['name'],*[value['name'] for value in row['versions'].values()]]))
        for name in names:
            declarations.extend([f"    {name} = {row['code']}", f'    {quoted(description(row,True))}'])
    declarations.append('')
    docs = ['# SQLAero error code registry','',banner,'',data['scope'],'',
            'A known code has one explicit maintained class. Unknown-code fallback uses only SQLSTATE classes 08/0A/22/23/42; all other, missing, or general states use DatabaseError. Actual ERR numbers and messages are retained.','',
            'Only functional names, codes and SQLSTATE facts are extracted from official material; Oracle message templates are not reproduced.','',
            '## Client codes','', '| Code | Member | Exact class | Meaning |','| --- | --- | --- | --- |']
    docs.extend(f"| {row['code']} | {row['name']} | {row['class']} | {row['meaning']} |" for row in client)
    docs.extend(['','## Server codes','', '| Code | Official symbol(s) | Exact class | Version / SQLSTATE | Meaning |','| --- | --- | --- | --- | --- |'])
    for row in server:
        names=list(dict.fromkeys(value['name'] for value in row['versions'].values()))
        versions=', '.join(f"{version}: {entry['sqlstate']}" for version,entry in row['versions'].items())
        docs.append(f"| {row['code']} | {' / '.join(names)} | {row['class']} | {versions} | {row['meaning']} |")
    docs.extend(['','## Frozen sources',''])
    docs.extend('- '+json.dumps(value,ensure_ascii=False) for value in data['sources'])
    outputs={ROOT/'crates/sqlaero-core/src/error.rs':'\n'.join(rust),ROOT/'crates/sqlaero-core/src/error/catalog.rs':'\n'.join(table),
             ROOT/'python/sqlaero/errors.pyi':'\n'.join(declarations),
             ROOT/'python/sqlaero/error_codes.pyi':('"""Public error-code identities, declared once in errors.pyi."""\n'
                 'from .errors import SQLAeroErrorCode as SQLAeroErrorCode, MySQLErrorCode as MySQLErrorCode\n'),
             ROOT/'docs/error-codes.md':'\n'.join(docs)+'\n'}
    # rustfmt is part of deterministic Rust output, not an independent registry.
    import subprocess
    for path,content in outputs.items():
        if path.suffix == '.rs':
            formatted=subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=content,text=True,capture_output=True,check=True)
            content=formatted.stdout
        if check:
            if not path.exists() or path.read_text(encoding='utf-8')!=content:
                raise ValueError(f'Generated registry output differs: {path}')
        else:
            path.write_text(content, encoding='utf-8', newline='\n')
    print(f"registry outputs {'verified' if check else 'written'}; {len(client)} client, {len(server)} server values")


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot',action='store_true',help='Freeze already obtained authoritative sources; no network or database calls.')
    parser.add_argument('--check',action='store_true',help='Verify outputs without changing them.')
    args=parser.parse_args()
    if args.snapshot:
        snapshot()
    generate(args.check)


if __name__=='__main__':
    main()
