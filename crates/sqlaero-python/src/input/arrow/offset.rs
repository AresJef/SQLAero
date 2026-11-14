//! Lazy public timezone protocols for Arrow UTC instants, only when a target
//! needs the original local representation. MySQL never prepares these APIs.

use super::{invalid, time_unit};
use crate::input::control::{self, Checkpoint};
use crate::{errors, input};
use arrow_schema::TimeUnit;
use pyo3::prelude::*;
use pyo3::types::{PyDateTime, PyDelta};
use std::sync::Arc;

enum Zone<'py> {
    Fixed(i128),
    Named(Bound<'py, PyAny>),
}

#[derive(Default)]
pub(super) struct Offsets<'py> {
    zones: Vec<(Arc<str>, Zone<'py>)>,
    api: Option<(Bound<'py, PyAny>, Bound<'py, PyAny>)>,
}

impl<'py> Offsets<'py> {
    pub(super) fn read(
        &mut self,
        py: Python<'py>,
        name: &Arc<str>,
        value: i64,
        unit: &TimeUnit,
        budget: &dyn Checkpoint,
    ) -> PyResult<i128> {
        // Schema clones share this immutable name owner. Pointer equality
        // avoids repeatedly scanning or copying an unbounded schema string.
        let mut found = None;
        for (index, (key, _)) in self.zones.iter().enumerate() {
            if index != 0 && index.is_multiple_of(64) {
                input::check(py, budget)?;
            }
            if Arc::ptr_eq(key, name) {
                found = Some(index);
                break;
            }
        }
        let index = if let Some(index) = found {
            index
        } else {
            let zone = if let Some(offset) = fixed(name) {
                Zone::Fixed(offset)
            } else {
                let name = input::unicode::from_utf8(py, name, budget)?;
                let module =
                    control::callback(py, Some(budget), || errors::module(py, c"zoneinfo"))?;
                let factory = control::callback(py, Some(budget), || {
                    errors::attribute(module.as_any(), c"ZoneInfo")
                })?;
                let zone = control::callback(py, Some(budget), || factory.call1((name,)))?;
                Zone::Named(zone)
            };
            self.zones
                .try_reserve(1)
                .map_err(|_| errors::memory_error(py))?;
            self.zones.push((name.clone(), zone));
            self.zones.len() - 1
        };
        let Zone::Named(zone) = &self.zones[index].1 else {
            let Zone::Fixed(offset) = self.zones[index].1 else {
                unreachable!()
            };
            return Ok(offset);
        };
        if self.api.is_none() {
            self.api = Some(control::callback(py, Some(budget), || {
                let base = py.get_type::<PyDateTime>();
                Ok((
                    errors::attribute(base.as_any(), c"fromtimestamp")?,
                    errors::attribute(base.as_any(), c"utcoffset")?,
                ))
            })?);
        }
        let (from_timestamp, utc_offset) = self
            .api
            .as_ref()
            .ok_or_else(|| invalid(py, "Arrow timezone source API was not prepared"))?;
        let unit = time_unit(unit);
        let divisor = match unit {
            sqlaero_transcode::temporal::TimeUnit::Second => 1,
            sqlaero_transcode::temporal::TimeUnit::Millisecond => 1000,
            sqlaero_transcode::temporal::TimeUnit::Microsecond => 1_000_000,
            sqlaero_transcode::temporal::TimeUnit::Nanosecond => 1_000_000_000,
            _ => unreachable!(),
        };
        // Resolve the actual UTC second; no binary64 epoch rounding or
        // nanosecond loss is used for the final native calendar conversion.
        let seconds = value.div_euclid(divisor);
        let local = control::callback(py, Some(budget), || from_timestamp.call1((seconds, zone)))?;
        let offset = control::callback(py, Some(budget), || utc_offset.call1((local,)))?;
        let micros = input::delta(offset.cast::<PyDelta>()?);
        if micros.unsigned_abs() >= 86_400_000_000 {
            return Err(invalid(
                py,
                "Arrow UTC offset must be strictly within one day",
            ));
        }
        Ok(micros)
    }
}

fn fixed(name: &str) -> Option<i128> {
    if matches!(name, "UTC" | "Etc/UTC" | "GMT" | "Etc/GMT" | "Z") {
        return Some(0);
    }
    let bytes = name.as_bytes();
    if bytes.len() != 6 || !matches!(bytes[0], b'+' | b'-') || bytes[3] != b':' {
        return None;
    }
    let digits = [bytes[1], bytes[2], bytes[4], bytes[5]];
    if !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let hour = i128::from((bytes[1] - b'0') * 10 + bytes[2] - b'0');
    let minute = i128::from((bytes[4] - b'0') * 10 + bytes[5] - b'0');
    if hour >= 24 || minute >= 60 {
        return None;
    }
    let micros = (hour * 60 + minute) * 60_000_000;
    Some(if bytes[0] == b'-' { -micros } else { micros })
}
