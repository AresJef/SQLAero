//! One native adaptation writes both real-capacity validation and readable SQL.

use crate::input::sink::ParameterSink;
use sqlaero_core::bind::{
    Context,
    template::{SpliceBoundary, Template},
    value::{TextSink, ValueError, ValueRef, ValueSink},
    writer,
};
use sqlaero_core::protocol::{framing::WireError, prepared::Parameters};

pub(super) struct DisplaySink<'template> {
    native: Parameters,
    template: &'template Template,
    output: Vec<u8>,
    cursor: usize,
    splices: Option<Vec<SpliceBoundary>>,
}

impl ParameterSink for DisplaySink<'_> {
    fn expression_output(&mut self, context: &Context) -> Result<Option<&mut Vec<u8>>, ValueError> {
        // Keep ordinary neighbors subject to their native capacity checks.
        // An expression itself has no native parameter representation, so
        // only its position is counted in the local validation artifact.
        self.native.write(ValueRef::Null, context)?;
        self.before()?;
        Ok(Some(&mut self.output))
    }

    fn add_splices(&mut self, splices: Vec<SpliceBoundary>) -> Result<(), ValueError> {
        if let Some(previous) = &mut self.splices {
            previous
                .try_reserve(splices.len())
                .map_err(WireError::from)?;
            previous.extend(splices);
        } else {
            self.splices = Some(splices);
        }
        Ok(())
    }
}
impl<'template> DisplaySink<'template> {
    pub(super) fn new(
        template: &'template Template,
        expected: u16,
        limit: usize,
    ) -> Result<Self, ValueError> {
        let native = Parameters::new(0, expected, limit)?;
        let mut output = Vec::new();
        writer::append(&mut output, &[3], usize::MAX)?;
        Ok(Self {
            native,
            template,
            output,
            cursor: 0,
            splices: None,
        })
    }
    fn before(&mut self) -> Result<(), ValueError> {
        if self
            .template
            .append_until_slot(&mut self.cursor, &mut self.output, usize::MAX)?
            .is_none()
        {
            return Err(WireError::Malformed("prepared display position is unavailable").into());
        }
        if let Some(splices) = &mut self.splices {
            splices.try_reserve(1).map_err(WireError::from)?;
            splices.push(SpliceBoundary {
                offset: self.output.len(),
                fragment: false,
            });
        }
        Ok(())
    }
    pub(super) fn finish(self) -> Result<(Vec<u8>, Option<Vec<SpliceBoundary>>), ValueError> {
        let Self {
            native,
            template,
            mut output,
            mut cursor,
            splices,
        } = self;
        // This payload is a local validation artifact only. Neither statement
        // id zero nor any byte here is sent, and the native buffer is released
        // before final readable text allocation. Peak native + SQL cost remains.
        drop(native.finish()?);
        if template
            .append_until_slot(&mut cursor, &mut output, usize::MAX)?
            .is_some()
        {
            return Err(WireError::Malformed("prepared display has an unconsumed position").into());
        }
        Ok((output, splices))
    }
}
impl ValueSink for DisplaySink<'_> {
    type Context = Context;
    type Error = ValueError;
    fn write(&mut self, value: ValueRef<'_>, context: &Context) -> Result<(), ValueError> {
        self.native.write(value, context)?;
        self.before()?;
        TextSink::new(&mut self.output).write(value, context)
    }
    fn decimal_start(
        &mut self,
        header: sqlaero_transcode::decimal::Header,
        context: &Context,
    ) -> Result<Option<sqlaero_transcode::decimal::Layout>, ValueError> {
        let plan = self.native.decimal_start(header, context)?;
        self.before()?;
        TextSink::new(&mut self.output).decimal_start(header, context)?;
        Ok(plan)
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Context) -> Result<(), ValueError> {
        self.native.decimal_fragment(ascii, context)?;
        TextSink::new(&mut self.output).decimal_fragment(ascii, context)
    }
    fn decimal_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.native.decimal_end(context)?;
        TextSink::new(&mut self.output).decimal_end(context)
    }
    fn text_start(&mut self, length: Option<usize>, context: &Context) -> Result<(), ValueError> {
        self.native.text_start(length, context)?;
        self.before()?;
        TextSink::new(&mut self.output).text_start(length, context)
    }
    fn text_fragment(&mut self, text: &str, context: &Context) -> Result<(), ValueError> {
        self.native.text_fragment(text, context)?;
        TextSink::new(&mut self.output).text_fragment(text, context)
    }
    fn text_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.native.text_end(context)?;
        TextSink::new(&mut self.output).text_end(context)
    }
    fn bytes_start(&mut self, length: usize, context: &Context) -> Result<(), ValueError> {
        self.native.bytes_start(length, context)?;
        self.before()?;
        TextSink::new(&mut self.output).bytes_start(length, context)
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Context) -> Result<(), ValueError> {
        self.native.bytes_fragment(bytes, context)?;
        TextSink::new(&mut self.output).bytes_fragment(bytes, context)
    }
    fn bytes_end(&mut self, context: &Context) -> Result<(), ValueError> {
        self.native.bytes_end(context)?;
        TextSink::new(&mut self.output).bytes_end(context)
    }
}
