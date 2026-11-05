//! Boundary destinations for the single scalar/provider recognition path.

use super::source::{CellWriter, PreparedSource};
use super::{binary, codec_error, scalar_error, text};
use crate::errors;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{TextSink, ValueError, ValueRef, ValueSink};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::protocol::framing::WireError;
use sqlaero_core::transport::deadline::IoBudget;

/// A destination receives an adapted value immediately. Python-specific text
/// diagnostics and signal checks remain at this boundary.
pub(crate) trait Destination {
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError>;

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()>;

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()>;

    fn text_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError>;
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError>;
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError>;
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError>;
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError>;
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError>;

    /// SQL fragments belong only to the text destination. A prepared position
    /// cannot contain nested rows, sequence syntax or cached SQL literals.
    fn fragment(&mut self, py: Python<'_>, bytes: &[u8], context: &Context) -> PyResult<()>;

    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()>;
    fn source_cell<'py>(
        &mut self,
        source: &mut dyn PreparedSource<'py>,
        row: usize,
        column: usize,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()>;

    /// Existing text-only dictionary caches remain on the text path.
    fn text_buffer(&mut self) -> Option<&mut Vec<u8>>;

    /// Necessary lower bounds, never a replacement for actual sink writes.
    fn check_text_minimum(&self, characters: usize, context: &Context) -> Result<(), WireError>;
    fn check_binary_width(&self, length: usize, context: &Context) -> Result<(), WireError>;
}

impl Destination for Vec<u8> {
    #[inline]
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).write(value, context)
    }

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        text(source, self, context, true, budget)
    }

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        binary(py, source, self, context.limit, budget)
    }
    fn text_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).text_start(length, context)
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).text_fragment(text, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).bytes_fragment(bytes, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).bytes_end(context)
    }
    fn fragment(&mut self, py: Python<'_>, bytes: &[u8], context: &Context) -> PyResult<()> {
        sqlaero_core::bind::writer::append(self, bytes, context.limit)
            .map_err(|error| errors::wire(py, error))
    }
    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()> {
        visitor.python(value, self, child)
    }
    fn source_cell<'py>(
        &mut self,
        source: &mut dyn PreparedSource<'py>,
        row: usize,
        column: usize,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        source.write_cell(row, column, self, context, budget, visitor)
    }
    fn text_buffer(&mut self) -> Option<&mut Vec<u8>> {
        Some(self)
    }
    fn check_text_minimum(&self, characters: usize, context: &Context) -> Result<(), WireError> {
        minimum(
            self.len()
                .checked_add(characters)
                .and_then(|n| n.checked_add(2)),
            context.limit,
        )
    }
    fn check_binary_width(&self, length: usize, context: &Context) -> Result<(), WireError> {
        sqlaero_core::bind::writer::binary_width(self.len(), length, context.limit).map(|_| ())
    }
}

pub(crate) struct Native<'a>(pub(crate) &'a mut dyn ValueSink<Error = ValueError>);

impl Destination for Native<'_> {
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.0.write(value, context)
    }

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        super::checkpoint(source.py(), budget)?;
        let value = super::unicode(source, context)?;
        let convert_error = |error, start| match error {
            ValueError::Encoding(error) => {
                let error = match error {
                    CodecError::Encode { offset, point } => CodecError::Encode {
                        offset: start + offset,
                        point,
                    },
                    error => error,
                };
                codec_error(source, value, context.codec.name(), error)
            }
            error => failure(source.py(), error),
        };
        if value.len() <= super::WORK_BYTES {
            self.0
                .write(ValueRef::Text(value), context)
                .map_err(|error| convert_error(error, 0))?;
        } else {
            self.0
                .text_start(value.len(), context)
                .map_err(|error| convert_error(error, 0))?;
            let mut start = 0;
            while start < value.len() {
                super::checkpoint(source.py(), budget)?;
                let mut end = (start + super::WORK_BYTES).min(value.len());
                while !value.is_char_boundary(end) {
                    end -= 1;
                }
                self.0
                    .text_fragment(&value[start..end], context)
                    .map_err(|error| convert_error(error, start))?;
                start = end;
            }
            self.0
                .text_end(context)
                .map_err(|error| convert_error(error, 0))?;
        }
        super::checkpoint(source.py(), budget)
    }

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        super::checkpoint(py, budget)?;
        if source.len() <= super::WORK_BYTES {
            self.0
                .write(ValueRef::Bytes(source), context)
                .map_err(|error| failure(py, error))?;
        } else {
            self.0
                .bytes_start(source.len(), context)
                .map_err(|error| failure(py, error))?;
            for chunk in source.chunks(super::WORK_BYTES) {
                super::checkpoint(py, budget)?;
                self.0
                    .bytes_fragment(chunk, context)
                    .map_err(|error| failure(py, error))?;
            }
            self.0
                .bytes_end(context)
                .map_err(|error| failure(py, error))?;
        }
        super::checkpoint(py, budget)
    }
    fn text_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.0.text_start(length, context)
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        self.0.text_fragment(text, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.0.bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        self.0.bytes_fragment(bytes, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.bytes_end(context)
    }
    fn fragment(&mut self, py: Python<'_>, _: &[u8], _: &Context) -> PyResult<()> {
        Err(fragment_error(py))
    }
    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()> {
        visitor.native_python(value, self.0, child)
    }
    fn source_cell<'py>(
        &mut self,
        source: &mut dyn PreparedSource<'py>,
        row: usize,
        column: usize,
        context: &Context,
        budget: &IoBudget,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        source.write_parameter_cell(row, column, self.0, context, budget, visitor)
    }
    fn text_buffer(&mut self) -> Option<&mut Vec<u8>> {
        None
    }
    fn check_text_minimum(&self, characters: usize, context: &Context) -> Result<(), WireError> {
        minimum(Some(characters), context.limit)
    }
    fn check_binary_width(&self, length: usize, context: &Context) -> Result<(), WireError> {
        minimum(Some(length), context.limit)
    }
}

fn minimum(required: Option<usize>, limit: usize) -> Result<(), WireError> {
    match required {
        Some(required) if required <= limit => Ok(()),
        required => Err(WireError::Limit {
            required: required.unwrap_or(usize::MAX),
            limit,
        }),
    }
}

pub(crate) fn fragment_error(py: Python<'_>) -> PyErr {
    errors::client(
        py,
        sqlaero_core::error::ClientErrorCode::UnsupportedCapability,
        "a prepared parameter must be one scalar value; nested SQL fragments are not supported",
    )
}

#[inline]
pub(crate) fn emit(
    destination: &mut (impl Destination + ?Sized),
    py: Python<'_>,
    value: ValueRef<'_>,
    context: &Context,
) -> PyResult<()> {
    destination
        .write(value, context)
        .map_err(|error| failure(py, error))
}

pub(crate) fn failure(py: Python<'_>, error: ValueError) -> PyErr {
    match error {
        ValueError::Scalar(error) => scalar_error(py, error),
        ValueError::Wire(error) => errors::wire(py, error),
        ValueError::Encoding(error) => {
            errors::builtin_message::<pyo3::exceptions::PyValueError>(py, error.to_string())
        }
    }
}

/// Statically select native integer width without allocating for small values.
pub(crate) trait NativeInteger {
    fn write_to(
        self,
        output: &mut (impl Destination + ?Sized),
        context: &Context,
    ) -> Result<(), ValueError>;
}
macro_rules! native_integers {
    ($variant:ident, $wide:ty, $($integer:ty),*) => {$(
        impl NativeInteger for $integer {
            fn write_to(self, output: &mut (impl Destination + ?Sized), context: &Context) -> Result<(), ValueError> {
                output.write(ValueRef::$variant(self as $wide), context)
            }
        }
    )*};
}
native_integers!(Signed, i64, i8, i16, i32, i64);
native_integers!(Unsigned, u64, u8, u16, u32, u64);
impl NativeInteger for i128 {
    fn write_to(
        self,
        output: &mut (impl Destination + ?Sized),
        context: &Context,
    ) -> Result<(), ValueError> {
        if let Ok(value) = i64::try_from(self) {
            output.write(ValueRef::Signed(value), context)
        } else if let Ok(value) = u64::try_from(self) {
            output.write(ValueRef::Unsigned(value), context)
        } else {
            output.write(
                ValueRef::BigInteger(&num_bigint::BigInt::from(self)),
                context,
            )
        }
    }
}
pub(crate) fn integer(
    output: &mut (impl Destination + ?Sized),
    value: impl NativeInteger,
    context: &Context,
) -> Result<(), ValueError> {
    value.write_to(output, context)
}
pub(crate) fn float(
    output: &mut (impl Destination + ?Sized),
    value: f64,
    context: &Context,
) -> Result<(), ValueError> {
    output.write(ValueRef::Float(value), context)
}
