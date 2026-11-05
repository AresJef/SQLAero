//! Checked binary rows borrowing the shared result's native byte owner.

use super::framing::WireError;
use crate::memory::Buffer;
use crate::result::text::{Cell, Column, ResultError};
use crate::result::value::FieldValue;

/// Validate one binary row and optionally store spans in its actual byte owner.
/// `base` is the row's absolute offset in that owner. Discarded rows undergo
/// exactly the same field and end-of-row checks without retaining locations.
///
/// # Errors
/// Returns malformed data, unsupported binary types or an actual quota error.
pub fn scan_row(
    data: &[u8],
    columns: &[Column],
    base: usize,
    mut fields: Option<&mut Buffer<Cell>>,
) -> Result<(), ResultError> {
    let initial = fields.as_ref().map_or(0, |fields| fields.len());
    let result = (|| {
        base.checked_add(data.len())
            .ok_or(WireError::Malformed("binary row offset overflow"))?;
        if data.first() != Some(&0) {
            return Err(WireError::Malformed("expected binary row marker").into());
        }
        let bitmap_length = columns
            .len()
            .checked_add(9)
            .ok_or(WireError::Malformed("binary NULL bitmap overflow"))?
            / 8;
        let mut position = 1 + bitmap_length;
        let bitmap = data
            .get(1..position)
            .ok_or(WireError::Malformed("truncated binary NULL bitmap"))?;
        if let Some(fields) = &mut fields {
            fields.reserve(columns.len())?;
        }
        for (index, column) in columns.iter().enumerate() {
            let bit = index + 2;
            let null = bitmap[bit / 8] & (1 << (bit % 8)) != 0 || column.type_code == 6;
            let field = if null {
                Cell::null()
            } else {
                let start = position;
                let (start, end) = match column.type_code {
                    1 => (start, advance(data, &mut position, 1)?),
                    2 | 13 => (start, advance(data, &mut position, 2)?),
                    3 | 4 | 9 => (start, advance(data, &mut position, 4)?),
                    5 | 8 => (start, advance(data, &mut position, 8)?),
                    7 | 10..=12 | 14 => {
                        let length = *data
                            .get(position)
                            .ok_or(WireError::Malformed("missing binary temporal length"))?;
                        (
                            start,
                            advance(data, &mut position, usize::from(length) + 1)?,
                        )
                    }
                    0 | 15 | 16 | 242 | 245..=255 => {
                        let length = length(data, &mut position)?;
                        let start = position;
                        (start, advance(data, &mut position, length)?)
                    }
                    _ => return Err(ResultError::Unsupported("unknown binary column type")),
                };
                // This also validates temporal lengths/components when the row
                // is discarded instead of retained for a later Decode call.
                value(column, &data[start..end])?;
                Cell::range(base + start, base + end)?
            };
            if let Some(fields) = &mut fields {
                fields.push(field)?;
            }
        }
        if position != data.len() {
            return Err(WireError::Malformed("unexpected trailing binary row bytes").into());
        }
        Ok(())
    })();
    if result.is_err()
        && let Some(fields) = fields
    {
        fields.truncate(initial);
    }
    result
}

/// Interpret a checked field span without converting native numbers to text.
/// Temporal spans include their length byte; length-encoded byte spans exclude
/// their prefix. The returned view cannot outlive the shared result byte owner.
///
/// # Errors
/// Rejects malformed field widths and unsupported binary column types.
pub fn value<'a>(column: &Column, raw: &'a [u8]) -> Result<FieldValue<'a>, WireError> {
    let unsigned = column.is_unsigned();
    Ok(match column.type_code {
        1 => {
            let [number] = array(raw)?;
            if unsigned {
                FieldValue::Unsigned(u64::from(number))
            } else {
                FieldValue::Signed(i64::from(number as i8))
            }
        }
        2 | 13 => {
            let number = u16::from_le_bytes(array(raw)?);
            if unsigned {
                FieldValue::Unsigned(u64::from(number))
            } else {
                FieldValue::Signed(i64::from(number as i16))
            }
        }
        3 | 9 => {
            let number = u32::from_le_bytes(array(raw)?);
            if unsigned {
                FieldValue::Unsigned(u64::from(number))
            } else {
                FieldValue::Signed(i64::from(number as i32))
            }
        }
        8 => {
            let number = u64::from_le_bytes(array(raw)?);
            if unsigned {
                FieldValue::Unsigned(number)
            } else {
                FieldValue::Signed(number as i64)
            }
        }
        4 => FieldValue::Float(f64::from(f32::from_le_bytes(array(raw)?))),
        5 => FieldValue::Float(f64::from_le_bytes(array(raw)?)),
        6 if raw.is_empty() => FieldValue::Null,
        7 | 10 | 12 | 14 => calendar(column.type_code, raw)?,
        11 => duration(raw)?,
        0 | 15 | 16 | 242 | 245..=255 => FieldValue::Bytes(raw),
        _ => {
            return Err(WireError::Malformed(
                "unsupported binary field type or width",
            ));
        }
    })
}

fn array<const N: usize>(raw: &[u8]) -> Result<[u8; N], WireError> {
    raw.try_into()
        .map_err(|_| WireError::Malformed("invalid binary numeric width"))
}

fn advance(data: &[u8], position: &mut usize, width: usize) -> Result<usize, WireError> {
    *position = position
        .checked_add(width)
        .filter(|end| *end <= data.len())
        .ok_or(WireError::Malformed("truncated binary field"))?;
    Ok(*position)
}

fn length(data: &[u8], position: &mut usize) -> Result<usize, WireError> {
    let marker = *data
        .get(*position)
        .ok_or(WireError::Malformed("missing binary field length"))?;
    *position += 1;
    let size = match marker {
        0..=250 => return Ok(usize::from(marker)),
        252 => 2,
        253 => 3,
        254 => 8,
        _ => return Err(WireError::Malformed("invalid binary field length marker")),
    };
    let start = *position;
    let end = advance(data, position, size)?;
    let mut bytes = [0; 8];
    bytes[..size].copy_from_slice(&data[start..end]);
    usize::try_from(u64::from_le_bytes(bytes))
        .map_err(|_| WireError::Malformed("binary field length overflows native index"))
}

fn calendar(kind: u8, raw: &[u8]) -> Result<FieldValue<'_>, WireError> {
    let length = *raw
        .first()
        .ok_or(WireError::Malformed("missing binary calendar length"))?;
    let date_only = matches!(kind, 10 | 14);
    if raw.len() != usize::from(length) + 1 || !matches!(length, 0 | 4 | 7 | 11) {
        return Err(WireError::Malformed("invalid binary calendar length"));
    }
    let (year, month, day) = if length >= 4 {
        (u16::from_le_bytes([raw[1], raw[2]]), raw[3], raw[4])
    } else {
        (0, 0, 0)
    };
    let (hour, minute, second) = if length >= 7 {
        (raw[5], raw[6], raw[7])
    } else {
        (0, 0, 0)
    };
    let microsecond = if length == 11 {
        u32::from_le_bytes(array(&raw[8..])?)
    } else {
        0
    };
    if microsecond > 999_999 {
        return Err(WireError::Malformed("invalid binary calendar microseconds"));
    }
    if date_only {
        return Ok(FieldValue::Date { year, month, day });
    }
    Ok(FieldValue::Datetime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        microsecond,
    })
}

fn duration(raw: &[u8]) -> Result<FieldValue<'_>, WireError> {
    let length = *raw
        .first()
        .ok_or(WireError::Malformed("missing binary TIME length"))?;
    if raw.len() != usize::from(length) + 1 || !matches!(length, 0 | 8 | 12) {
        return Err(WireError::Malformed("invalid binary TIME length"));
    }
    if length == 0 {
        return Ok(FieldValue::Duration(0));
    }
    let days = u32::from_le_bytes(array(&raw[2..6])?);
    let micros = if length == 12 {
        u32::from_le_bytes(array(&raw[9..])?)
    } else {
        0
    };
    if raw[1] > 1 || raw[6] > 23 || raw[7] > 59 || raw[8] > 59 || micros > 999_999 {
        return Err(WireError::Malformed("invalid binary TIME components"));
    }
    let seconds = ((i128::from(days) * 24 + i128::from(raw[6])) * 60 + i128::from(raw[7])) * 60
        + i128::from(raw[8]);
    let micros = seconds * 1_000_000 + i128::from(micros);
    Ok(FieldValue::Duration(if raw[1] == 1 {
        -micros
    } else {
        micros
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Pool, Scope};
    use crate::protocol::CapabilityFlags;
    use crate::result::text::{Receiver, Response};

    fn column(kind: u8, unsigned: bool) -> Column {
        let mut packet = Vec::new();
        for name in [b"def".as_slice(), b"", b"", b"", b"value", b""] {
            packet.push(name.len() as u8);
            packet.extend_from_slice(name);
        }
        packet.extend_from_slice(&[
            12,
            45,
            0,
            255,
            0,
            0,
            0,
            kind,
            u8::from(unsigned) * 32,
            0,
            6,
            0,
            0,
        ]);
        let mut receiver = Receiver::new(
            CapabilityFlags::CLIENT_PROTOCOL_41 | CapabilityFlags::CLIENT_DEPRECATE_EOF,
            Scope::new(Pool::new(1_000_000), 1_000_000).unwrap(),
            false,
        )
        .unwrap();
        receiver.push(&[1]).unwrap();
        receiver.push(&packet).unwrap();
        let Response::Rows(rows) = receiver.push(&[0xfe, 0, 0, 2, 0, 0, 0]).unwrap().unwrap()
        else {
            panic!("expected columns");
        };
        rows.columns[0]
    }

    #[test]
    fn row_spans_keep_nulls_empty_bytes_and_native_integer_bits_distinct() {
        let columns = [column(8, true), column(253, false), column(8, false)];
        let mut row = vec![0, 0b0001_0000];
        row.extend_from_slice(&u64::MAX.to_le_bytes());
        row.push(0);
        let mut fields = Buffer::new();
        scan_row(&row, &columns, 2, Some(&mut fields)).unwrap();
        let mut owner = vec![99, 99];
        owner.extend_from_slice(&row);
        assert_eq!(
            value(&columns[0], fields[0].get(&owner).unwrap().unwrap()).unwrap(),
            FieldValue::Unsigned(u64::MAX)
        );
        assert_eq!(fields[1].get(&owner).unwrap(), Some(b"".as_slice()));
        assert_eq!(fields[2].get(&owner).unwrap(), None);
        scan_row(&row, &columns, 0, None).unwrap();
    }

    #[test]
    fn fixed_width_values_apply_signedness_and_preserve_float_bits() {
        for (kind, bytes, expected) in [
            (1, vec![0xff], FieldValue::Signed(-1)),
            (2, vec![0xfe, 0xff], FieldValue::Signed(-2)),
            (9, (-3_i32).to_le_bytes().to_vec(), FieldValue::Signed(-3)),
            (
                8,
                i64::MIN.to_le_bytes().to_vec(),
                FieldValue::Signed(i64::MIN),
            ),
        ] {
            assert_eq!(value(&column(kind, false), &bytes).unwrap(), expected);
        }
        let FieldValue::Float(number) =
            value(&column(5, false), &(-0.0_f64).to_le_bytes()).unwrap()
        else {
            panic!("expected native float");
        };
        assert_eq!(number.to_bits(), (-0.0_f64).to_bits());
        assert_eq!(
            value(&column(4, false), &1.5_f32.to_le_bytes()).unwrap(),
            FieldValue::Float(1.5)
        );
    }

    #[test]
    fn temporal_values_keep_calendar_components_and_exact_negative_duration() {
        assert_eq!(
            value(&column(10, false), &[0]).unwrap(),
            FieldValue::Date {
                year: 0,
                month: 0,
                day: 0
            }
        );
        assert_eq!(
            value(&column(12, false), &[4, 0xea, 7, 0, 31]).unwrap(),
            FieldValue::Datetime {
                year: 2026,
                month: 0,
                day: 31,
                hour: 0,
                minute: 0,
                second: 0,
                microsecond: 0
            }
        );
        let mut time = vec![12, 1, 2, 0, 0, 0, 3, 4, 5];
        time.extend_from_slice(&123_456_u32.to_le_bytes());
        assert_eq!(
            value(&column(11, false), &time).unwrap(),
            FieldValue::Duration(-183_845_123_456)
        );
    }

    #[test]
    fn malformed_rows_are_rejected_even_when_discarded() {
        let columns = [column(253, false)];
        for data in [
            vec![],
            vec![1, 0, 0],
            vec![0],
            vec![0, 0, 0xfb],
            vec![0, 0, 4, b'a'],
            vec![0, 0, 0, 99],
        ] {
            assert!(scan_row(&data, &columns, 0, None).is_err(), "{data:?}");
        }
        for data in [vec![0, 0, 1], vec![0, 0, 5, 1, 2, 3, 4, 5]] {
            assert!(scan_row(&data, &[column(10, false)], 0, None).is_err());
        }
        assert!(scan_row(&[0, 0, 0], &[column(0xaa, false)], 0, None).is_err());
        assert!(scan_row(&[0, 0, 0], &columns, usize::MAX, None).is_err());
    }
}
