//! Compile observed mapping facts into the minimal pinned-component deltas.

use sqlaero_core::charset::multibyte::{base_decode, base_encode};
use sqlaero_core::charset::{self, CodecRoute};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write;
use std::path::PathBuf;

type Mapping = BTreeMap<u32, u32>;
type Route = (Mapping, Mapping, [u8; 256]);

fn unhex(value: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !value.len().is_multiple_of(2) {
        return Err("odd hex width".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
        .collect()
}
fn key(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0, |value, byte| value << 8 | u32::from(*byte))
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected repository root")?,
    );
    let directory = root.join("evidence/rust-sync-kernel/charset");
    let mut routes = BTreeMap::<usize, Route>::new();
    for file in std::fs::read_dir(directory)? {
        let file = file?;
        let name = file.file_name();
        let name = name.to_str().ok_or("UTF-8 observation filename")?;
        let Some(name) = name.strip_suffix("-observed.tsv") else {
            continue;
        };
        let CodecRoute::Multibyte(kind) = charset::resolve(Some(name), None)?.charset.route else {
            return Err("expected a multibyte route".into());
        };
        let mut decode = Mapping::new();
        let mut canonical = Mapping::new();
        let mut widths = [0u8; 256];
        for line in std::fs::read_to_string(file.path())?.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() != 3 {
                return Err("malformed observation".into());
            }
            let bytes = unhex(fields[0])?;
            let expected = if fields[1] == "-" {
                None
            } else {
                let utf8 = unhex(fields[1])?;
                let text = std::str::from_utf8(&utf8)?;
                let mut chars = text.chars();
                let point = chars.next().ok_or("empty expected character")?;
                if chars.next().is_some() {
                    return Err("multiple expected characters".into());
                }
                Some(point)
            };
            if expected != base_decode(kind, &bytes) {
                decode.insert(key(&bytes), expected.map(u32::from).unwrap_or(u32::MAX));
            }
            if let Some(point) = expected {
                widths[usize::from(bytes[0])] |= 1 << bytes.len();
                if fields[2] != "-" {
                    let encoded = key(&unhex(fields[2])?);
                    if let Some(old) = canonical.insert(u32::from(point), encoded)
                        && old != encoded
                    {
                        return Err("inconsistent canonical encoding".into());
                    }
                }
            }
        }
        canonical.retain(|point, encoded| {
            char::from_u32(*point)
                .and_then(|point| base_encode(kind, point))
                .map(|value| value.key())
                != Some(*encoded)
        });
        println!(
            "{name}: {} decode/validity deltas, {} canonical encoder deltas",
            decode.len(),
            canonical.len()
        );
        routes.insert(kind as usize, (decode, canonical, widths));
    }
    if routes.len() != 9 {
        return Err("incomplete multibyte routes".into());
    }
    let mut output = String::from(
        "//! Observed MySQL 8.4.13/9.7.4 differences from encoding_rs 0.8.42.\n//! Generated from SQL results by tools/charset_tables.py and this crate's\n//! charset_delta example; no server source or private component table copied.\n\n",
    );
    for (name, position) in [("DECODE", 0), ("ENCODE", 1)] {
        writeln!(output, "pub(super) static {name}: [&[(u32, u32)]; 9] = [")?;
        for index in 0..9 {
            let route = &routes[&index];
            let entries = if position == 0 { &route.0 } else { &route.1 };
            writeln!(output, "    &[")?;
            for (source, target) in entries {
                writeln!(output, "        (0x{source:X}, 0x{target:X}),")?;
            }
            writeln!(output, "    ],")?;
        }
        writeln!(output, "];\n")?;
    }
    writeln!(output, "pub(super) static WIDTHS: [[u8; 256]; 9] = [")?;
    for route in routes.values() {
        writeln!(output, "    {:?},", route.2)?;
    }
    writeln!(output, "];")?;
    std::fs::write(
        root.join("crates/sqlaero-core/src/charset/multibyte_delta.rs"),
        output,
    )?;
    Ok(())
}
