//! Boundary destinations for the single scalar/provider recognition path.

use super::source::{CellWriter, PreparedSource};
use super::{binary, scalar_error};
use crate::errors;
use crate::input::control::Checkpoint;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{TextSink, ValueError, ValueRef, ValueSink};
use sqlaero_core::charset::codec::CodecError;
use sqlaero_core::protocol::framing::WireError;

/// A destination receives an adapted value immediately. Python-specific text
/// diagnostics and signal checks remain at this boundary.
pub(crate) trait Destination {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        context: &Context,
        budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()>;
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError>;
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError>;
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError>;
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError>;

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()>;

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()>;

    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError>;
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError>;
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError>;
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError>;
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError>;
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError>;

    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()>;

    /// Existing text-only dictionary caches remain on the text path.
    fn text_buffer(&mut self) -> Option<&mut Vec<u8>>;

    /// Necessary lower bounds, never a replacement for actual sink writes.
    fn check_text_minimum(&self, characters: usize, context: &Context) -> Result<(), WireError>;
    fn check_binary_width(&self, length: usize, context: &Context) -> Result<(), WireError>;
}

impl Destination for Vec<u8> {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        _context: &Context,
        _budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        visitor.source(source, self).map(|_| ())
    }
    #[inline(always)]
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).write(value, context)
    }
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        TextSink::new(self).decimal_start(header, context)
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).decimal_fragment(ascii, context)
    }
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self).decimal_end(context)
    }

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        super::unicode::write(
            source,
            &mut TextSink::new(self),
            context,
            budget,
            context.codec.name(),
            |error, start, fragment| unicode_failure(source, context, error, start, fragment),
        )
    }

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        binary(py, source, self, context.limit, budget)
    }
    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError> {
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
    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()> {
        visitor.python(value, self, child)
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

/// Borrow a boundary destination as the same native Consumer contract. This
/// forwards facts/fragments only; it never recognizes a Python input again.
pub(super) struct Adapter<'a, D: Destination + ?Sized>(pub(super) &'a mut D);
impl<D: Destination + ?Sized> ValueSink for Adapter<'_, D> {
    type Context = Context;
    type Error = ValueError;
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.0.write(value, context)
    }
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        self.0.decimal_start(header, context)
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError> {
        self.0.decimal_fragment(ascii, context)
    }
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.decimal_end(context)
    }
    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError> {
        self.0.text_start(length, context)
    }
    fn text_fragment(&mut self, value: &str, context: &Context) -> Result<(), ValueError> {
        self.0.text_fragment(value, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.0.bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, value: &[u8], context: &Context) -> Result<(), ValueError> {
        self.0.bytes_fragment(value, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.bytes_end(context)
    }
}

pub(crate) struct Native<'a>(
    pub(crate) &'a mut dyn ValueSink<Context = sqlaero_core::bind::Context, Error = ValueError>,
);

impl Destination for Native<'_> {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        context: &Context,
        budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        if source.shape() != super::source::Shape::Scalar {
            // Preserve source-boundary checks before the prepared policy
            // rejects a fragment. In particular, an OUT placeholder cannot
            // become acceptable merely because a nested value contains NULL.
            visitor.enter_native(source.owner().py())?;
            visitor.leave_native();
            return Err(fragment_error(source.owner().py()));
        }
        source.write_parameter_cell(0, 0, self.0, context, budget, visitor)
    }
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.0.write(value, context)
    }
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        self.0.decimal_start(header, context)
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError> {
        self.0.decimal_fragment(ascii, context)
    }
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.0.decimal_end(context)
    }

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        super::unicode::write(
            source,
            self.0,
            context,
            budget,
            context.codec.name(),
            |error, start, fragment| unicode_failure(source, context, error, start, fragment),
        )
    }

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&dyn Checkpoint>,
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
    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError> {
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
    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()> {
        visitor.native_python(value, self.0, child)
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

// Keep this thin error bridge visible together with the destination so a
// statically known value does not regain a generic dispatch at its caller.
#[inline(always)]
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

fn unicode_failure(
    source: &Bound<'_, PyString>,
    context: &Context,
    error: ValueError,
    start: usize,
    fragment: &str,
) -> PyErr {
    match error {
        ValueError::Encoding(CodecError::Encode { offset, point }) => {
            let position = start + fragment[..offset].chars().count();
            match errors::message(
                source.py(),
                format_args!(
                    "character U+{point:04X} is not representable in MySQL {}",
                    context.codec.name()
                ),
            ) {
                Ok(reason) => super::encode_error(
                    source,
                    context.codec.name(),
                    position,
                    position + 1,
                    &reason,
                ),
                Err(error) => error,
            }
        }
        error => failure(source.py(), error),
    }
}

pub(crate) fn failure(py: Python<'_>, error: ValueError) -> PyErr {
    match error {
        ValueError::UtcRequired => errors::client(
            py,
            sqlaero_core::error::ClientErrorCode::SessionState,
            error.to_string(),
        ),
        ValueError::Scalar(error) => scalar_error(py, error),
        ValueError::Calendar(error) => errors::builtin_format::<pyo3::exceptions::PyValueError>(
            py,
            format_args!("invalid calendar components with UTC offset: {error}"),
        ),
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

/// Observe only explicit writer operations, never parse escaped SQL bytes.
/// This destination is instantiated solely for a whole-row layout proof;
/// ordinary text and native parameter hot paths keep their static sinks.
pub(crate) struct RoleSink<'a> {
    output: &'a mut Vec<u8>,
    role: Option<super::source::FragmentRole>,
}
impl<'a> RoleSink<'a> {
    pub(crate) fn new(output: &'a mut Vec<u8>) -> Self {
        Self { output, role: None }
    }
    pub(crate) fn role(&self) -> super::source::FragmentRole {
        self.role.unwrap_or(super::source::FragmentRole::Scalar)
    }
    fn observe(&mut self, role: super::source::FragmentRole) {
        self.role.get_or_insert(role);
    }
    fn scalar(&mut self) {
        self.observe(super::source::FragmentRole::Scalar);
    }
}
impl Destination for RoleSink<'_> {
    fn source(
        &mut self,
        source: &mut dyn PreparedSource<'_>,
        _context: &Context,
        _budget: &dyn Checkpoint,
        visitor: &mut dyn CellWriter,
    ) -> PyResult<()> {
        let role = visitor.source(source, self.output)?;
        self.observe(role);
        Ok(())
    }
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.scalar();
        self.output.write(value, context)
    }
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        self.scalar();
        self.output.decimal_start(header, context)
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError> {
        self.output.decimal_fragment(ascii, context)
    }
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.output.decimal_end(context)
    }
    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        self.scalar();
        self.output.text(source, context, budget)
    }
    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&dyn Checkpoint>,
    ) -> PyResult<()> {
        self.scalar();
        self.output.binary(py, source, context, budget)
    }
    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError> {
        self.scalar();
        self.output.text_start(length, context)
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        self.output.text_fragment(text, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.output.text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.scalar();
        self.output.bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        self.output.bytes_fragment(bytes, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.output.bytes_end(context)
    }
    fn python(
        &mut self,
        visitor: &mut dyn CellWriter,
        value: &Bound<'_, PyAny>,
        child: Option<usize>,
    ) -> PyResult<()> {
        let role = visitor.python_role(value, self.output, child)?;
        self.observe(role);
        Ok(())
    }
    fn text_buffer(&mut self) -> Option<&mut Vec<u8>> {
        None
    }
    fn check_text_minimum(&self, characters: usize, context: &Context) -> Result<(), WireError> {
        self.output.check_text_minimum(characters, context)
    }
    fn check_binary_width(&self, length: usize, context: &Context) -> Result<(), WireError> {
        self.output.check_binary_width(length, context)
    }
}
