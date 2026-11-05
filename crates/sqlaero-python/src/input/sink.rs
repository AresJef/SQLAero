//! Boundary destinations for the single scalar/provider recognition path.

use super::{binary, codec_error, scalar_error, text};
use crate::errors;
use pyo3::prelude::*;
use pyo3::types::PyString;
use sqlaero_core::bind::Context;
use sqlaero_core::bind::value::{TextSink, ValueError, ValueRef, ValueSink};
use sqlaero_core::charset::codec::CodecError;
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
}

pub(crate) struct Text<'a>(pub(crate) &'a mut Vec<u8>);

impl Destination for Text<'_> {
    #[inline]
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        TextSink::new(self.0).write(value, context)
    }

    fn text(
        &mut self,
        source: &Bound<'_, PyString>,
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        text(source, self.0, context, true, budget)
    }

    fn binary(
        &mut self,
        py: Python<'_>,
        source: &[u8],
        context: &Context,
        budget: Option<&IoBudget>,
    ) -> PyResult<()> {
        binary(py, source, self.0, context.limit, budget)
    }
}

pub(crate) struct Native<'a, S: ValueSink<Error = ValueError>>(pub(crate) &'a mut S);

impl<S: ValueSink<Error = ValueError>> Destination for Native<'_, S> {
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
