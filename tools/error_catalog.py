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
# These explicit known-condition decisions precede SQLSTATE categorization.
OVERRIDES: dict[str, str] = {
    'ER_DUP_ENTRY': 'IntegrityError', 'ER_DUP_KEY': 'IntegrityError',
    'ER_CHECK_CONSTRAINT_VIOLATED': 'IntegrityError', 'ER_NO_DEFAULT_FOR_FIELD': 'IntegrityError',
    'ER_FOREIGN_DUPLICATE_KEY_WITH_CHILD_INFO': 'IntegrityError',
    'ER_FOREIGN_DUPLICATE_KEY_WITHOUT_CHILD_INFO': 'IntegrityError',
    'ER_LOCK_DEADLOCK': 'OperationalError', 'ER_LOCK_WAIT_TIMEOUT': 'OperationalError',
    'ER_QUERY_INTERRUPTED': 'OperationalError', 'ER_QUERY_TIMEOUT': 'OperationalError',
    'ER_ACCESS_DENIED_ERROR': 'OperationalError', 'ER_ACCESS_DENIED_NO_PASSWORD_ERROR': 'OperationalError',
    'ER_DBACCESS_DENIED_ERROR': 'OperationalError', 'ER_TABLEACCESS_DENIED_ERROR': 'OperationalError',
    'ER_COLUMNACCESS_DENIED_ERROR': 'OperationalError', 'ER_PROCACCESS_DENIED_ERROR': 'OperationalError',
    'ER_SPECIFIC_ACCESS_DENIED_ERROR': 'OperationalError', 'ER_SESSION_WAS_KILLED': 'OperationalError',
    'ER_CON_COUNT_ERROR': 'OperationalError', 'ER_USER_LIMIT_REACHED': 'OperationalError',
    'ER_TOO_MANY_USER_CONNECTIONS': 'OperationalError', 'ER_OUT_OF_RESOURCES': 'OperationalError',
    'ER_OUTOFMEMORY': 'OperationalError', 'ER_DISK_FULL': 'OperationalError',
    'ER_RECORD_FILE_FULL': 'OperationalError', 'ER_STACK_OVERRUN': 'OperationalError',
    'ER_STACK_OVERRUN_NEED_MORE': 'OperationalError', 'ER_LOCK_TABLE_FULL': 'OperationalError',
    'ER_CANT_CREATE_FILE': 'OperationalError', 'ER_CANT_CREATE_TABLE': 'OperationalError',
    'ER_CANT_CREATE_DB': 'OperationalError', 'ER_CANT_OPEN_FILE': 'OperationalError',
    'ER_CANT_READ_DIR': 'OperationalError', 'ER_ERROR_ON_READ': 'OperationalError',
    'ER_ERROR_ON_WRITE': 'OperationalError', 'ER_FILE_NOT_FOUND': 'OperationalError',
    'ER_CANT_GET_STAT': 'OperationalError', 'ER_CANT_LOCK': 'OperationalError',
    'ER_NOT_SUPPORTED_YET': 'NotSupportedError', 'ER_UNSUPPORTED_EXTENSION': 'NotSupportedError',
    'ER_FEATURE_DISABLED': 'NotSupportedError', 'ER_UNKNOWN_STORAGE_ENGINE': 'NotSupportedError',
    'ER_PLUGIN_IS_NOT_LOADED': 'NotSupportedError',
    'ER_INTERNAL_ERROR': 'InternalError', 'ER_NOT_KEYFILE': 'InternalError',
    'ER_CRASHED_ON_USAGE': 'InternalError', 'ER_CRASHED_ON_REPAIR': 'InternalError',
    'ER_WARN_DATA_OUT_OF_RANGE': 'DataError', 'WARN_DATA_TRUNCATED': 'DataError',
    'ER_TRUNCATED_WRONG_VALUE': 'DataError', 'ER_TRUNCATED_WRONG_VALUE_FOR_FIELD': 'DataError',
    'ER_DATA_TOO_LONG': 'DataError', 'ER_WARN_INVALID_TIMESTAMP': 'DataError',
    'ER_INVALID_CHARACTER_STRING': 'DataError', 'ER_ILLEGAL_VALUE_FOR_TYPE': 'DataError',
    'ER_DIVISION_BY_ZERO': 'DataError', 'ER_WRONG_VALUE_FOR_TYPE': 'DataError',
    'ER_WRONG_VALUE': 'DataError', 'ER_WRONG_TYPE_FOR_VAR': 'DataError',
    'ER_WRONG_VALUE_FOR_VAR': 'DataError', 'ER_WARN_NULL_TO_NOTNULL': 'IntegrityError',
    'EE_UNKNOWN_CHARSET': 'NotSupportedError', 'EE_UNKNOWN_COLLATION': 'NotSupportedError',
    'EE_COLLATION_ALIAS_ERROR': 'NotSupportedError',
}
CLASS_BY_STATE: dict[str, str] = {
    '08':'OperationalError', '0A':'NotSupportedError', '22':'DataError',
    '23':'IntegrityError', '42':'ProgrammingError',
    # Known catalog conditions have these additional explicit categories;
    # unknown-code fallback intentionally does not include them.
    '07':'ProgrammingError', '21':'ProgrammingError', '24':'ProgrammingError',
    '25':'ProgrammingError', '28':'OperationalError', '40':'OperationalError',
}
WORDS = {'DB':'database','DBS':'databases','CANT':'cannot','COL':'column','COLS':'columns',
         'COLL':'collation','CHARSET':'character set','COLLATION':'collation','NUM':'number',
         'ARG':'argument','ARGS':'arguments','VAR':'variable','VARS':'variables','FUNC':'function',
         'FK':'foreign key','PK':'primary key','REF':'reference','TRX':'transaction','TX':'transaction',
         'OUTOFMEMORY':'out of memory','OOM':'out of memory','STMT':'statement',
         'WRONG':'invalid','NDB':'NDB','SQL':'SQL','JSON':'JSON','XML':'XML','GTID':'GTID',
         'SSL':'SSL','TLS':'TLS','IO':'I/O','DDL':'DDL','DML':'DML','UDF':'UDF','XA':'XA',
         'PTHREAD':'POSIX thread','LH':'Lakehouse','SRS':'spatial reference system'}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def client_inputs() -> list[dict[str, Any]]:
    """Capture existing registered meanings without renumbering a used code."""
    source = (ROOT / 'crates/sqlaero-core/src/error.rs').read_text()
    values: list[dict[str, Any]] = []
    for doc, name, number in re.findall(r'    /// ([^\n]+)\n    (\w+) = ([\d_]+),', source):
        category, meaning = doc.split(': ', 1)
        if category not in CLASSES:
            raise ValueError(f'Unknown client category: {category}')
        public = re.sub(r'(?<=[a-z0-9])(?=[A-Z])', '_', name).upper()
        values.append({'code': int(number.replace('_','')), 'rust_name': name,
                       'name': public, 'class': category, 'meaning': meaning})
    if len(values) != 19:
        raise ValueError('Expected the 19 already implemented client codes')
    return values


def installed(version: str, label: str) -> tuple[dict[int, dict[str, str]], list[dict[str, str]], list[dict[str, Any]]]:
    source = Path(f'/opt/homebrew/opt/mysql@{version}/share/mysql/messages_to_clients.txt')
    header = Path(f'/opt/homebrew/opt/mysql@{version}/include/mysql/mysqld_error.h')
    observed_path = EVIDENCE / f'error-catalog-{label}-observed.json'
    observed: dict[int, tuple[str, str]] = {
        row[0]: (row[1], row[2]) for row in json.loads(observed_path.read_text()) if row[0] is not None
    }
    definitions = {name: int(code) for name, code in re.findall(r'^#define (\w+) (\d+)$', header.read_text(), re.M)}
    codes: dict[int, dict[str, str]] = {}
    excluded: list[dict[str, Any]] = []
    number = 1000
    for line in source.read_text().splitlines():
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
                codes[number] = {'name':symbol,'sqlstate':state}
            number += 1
    sources = [{'kind':kind,'path':str(path),'sha256':digest(path)} for kind,path in (
        ('official installed client messages',source),('official installed generated header',header),('actual dedicated-server registration',observed_path))]
    return codes, sources, excluded


def globals_for(label: str) -> tuple[dict[int, dict[str, str]], dict[str, str]]:
    path = EVIDENCE / f'global-errors-{label}.html'
    text = unescape(re.sub(r'<[^>]*>', ' ', path.read_text()))
    pairs = re.findall(r'Error number:\s*(\d+)\s*;\s*Symbol:\s*([A-Z][A-Z_0-9]+)', text)
    values = {int(code): {'name':symbol,'sqlstate':'HY000'} for code,symbol in pairs}
    if not values or any(code < 1 or code > 999 for code in values):
        raise ValueError('Invalid official global-error facts')
    version = '8.4' if label == '84' else '9.7'
    return values, {'kind':'official shared global error reference',
                    'url':f'https://dev.mysql.com/doc/mysql-errors/{version}/en/global-error-reference.html',
                    'path':str(path),'sha256':digest(path)}


def classify(symbol: str, state: str) -> tuple[str, str]:
    if symbol in OVERRIDES:
        return OVERRIDES[symbol], 'Explicit known-condition semantic decision, ahead of SQLSTATE.'
    if symbol.startswith('EE_'):
        return 'OperationalError', 'Known shared system/file/resource operation condition.'
    if state[:2] in CLASS_BY_STATE:
        return CLASS_BY_STATE[state[:2]], f'Explicit known-condition SQLSTATE class {state[:2]}.'
    return 'DatabaseError', 'Known general database condition without a reliably narrower DB-API category.'


def meaning(symbol: str) -> str:
    words = re.sub(r'^(?:ER|EE|WARN)_', '', symbol).split('_')
    return ' '.join(WORDS.get(word, word.lower()) for word in words)


def snapshot() -> None:
    versions: dict[str, dict[int, dict[str, str]]] = {}
    sources: list[dict[str, str]] = []
    excluded: list[dict[str, Any]] = []
    for version,label in [('8.4','84'),('9.7','97')]:
        codes, provenance, omitted = installed(version,label)
        shared, source = globals_for(label)
        versions[version] = codes | shared
        sources.extend(provenance + [source])
        excluded.extend(omitted)
    server: list[dict[str, Any]] = []
    all_codes: set[int] = set()
    for values in versions.values():
        all_codes.update(values)
    for code in sorted(all_codes):
        conditions = {version: values[code] for version,values in versions.items() if code in values}
        decisions = {classify(value['name'],value['sqlstate'])[0] for value in conditions.values()}
        if len(decisions) != 1:
            raise ValueError(f'Version-specific class decision required for {code}: {conditions}')
        latest = conditions.get('9.7',next(iter(conditions.values())))
        category,basis = classify(latest['name'],latest['sqlstate'])
        server.append({'code':code,'name':latest['name'],'class':category,'meaning':meaning(latest['name']),
                       'basis':basis,'versions':conditions})
    data = {'schema':1,'recorded_at':datetime.now(timezone.utc).isoformat(),
            'scope':'Registered official client-message conditions plus applicable shared globals; no log-only MY or other libraries CR codes. Warning/Note responses never automatically raise; mappings apply only to an actual protocol ERR.',
            'versions':{'8.4':'8.4.13','9.7':'9.7.4'},'sources':sources,
            'client':client_inputs(),'server':server,'excluded':excluded}
    REGISTRY.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'server_union':len(server),'per_version':{version:len(values) for version,values in versions.items()},
                      'classes':dict(Counter(row['class'] for row in server))},sort_keys=True))


def quoted(value: str) -> str:
    return json.dumps(value,ensure_ascii=False)


def description(row: dict[str, Any], server: bool) -> str:
    if not server:
        return f"{row['meaning'].capitalize()} Code {row['code']}; client origin (is_server_error=False); exact exception class {row['class']}."
    variants = '; '.join(f"MySQL {version}: {entry['name']} ({entry['sqlstate']})" for version,entry in row['versions'].items())
    return f"Server condition: {row['meaning']}. Code {row['code']}; direct server ERR origin (is_server_error=True); exact exception class {row['class']}. {variants}. This mapping applies only to ERR, not ordinary Warning/Note responses."


def generate(check: bool) -> None:
    data: dict[str, Any] = json.loads(REGISTRY.read_text())
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
    declarations = ['"""'+banner+'"""','from enum import IntEnum','', 'class SQLAeroErrorCode(IntEnum):',
                    '    """Stable SQLAero client-origin codes; each member documents its exact class."""']
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
             ROOT/'python/sqlaero/error_codes.pyi':'\n'.join(declarations),ROOT/'docs/error-codes.md':'\n'.join(docs)+'\n'}
    # rustfmt is part of deterministic Rust output, not an independent registry.
    import subprocess
    for path,content in outputs.items():
        if path.suffix == '.rs':
            formatted=subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=content,text=True,capture_output=True,check=True)
            content=formatted.stdout
        if check:
            if not path.exists() or path.read_text()!=content:
                raise ValueError(f'Generated registry output differs: {path}')
        else:
            path.write_text(content)
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
