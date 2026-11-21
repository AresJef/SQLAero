//! Bounded JSON fragments compose with a native text destination at the boundary.
//!
//! No Python protocol or signal handler runs while borrowed native facts are
//! being written. The existing source driver retains all original work checks.

use crate::input::cell::NativeFailure;
use crate::input::sink::Destination;
use sqlaero_core::bind::Context;
use sqlaero_core::json::{DirectKey, Error, Options, Writer};
use sqlaero_transcode::decimal::{Header, Layout};
use sqlaero_transcode::value::{Consumer, ScalarRef};

impl From<Error> for NativeFailure {
    fn from(error: Error) -> Self {
        Self::Json(error)
    }
}

struct Stream<'a, D: Destination + ?Sized> {
    output: &'a mut D,
    context: &'a Context,
}

/// The same JSON traversal either retains its document or drains native text
/// fragments. A streaming failure never publishes a request or resets state.
pub(super) struct Output<'a, D: Destination + ?Sized = Vec<u8>> {
    writer: Writer,
    stream: Option<Stream<'a, D>>,
}

macro_rules! structural {
    ($name:ident $(, $argument:ident: $kind:ty)*) => {
        pub(super) fn $name(&mut self, $($argument: $kind),*) -> Result<(), NativeFailure> {
            self.apply(|writer| writer.$name($($argument),*))
        }
    };
}

impl<'a, D: Destination + ?Sized> Output<'a, D> {
    pub(super) fn buffered() -> Self {
        Self {
            writer: Writer::new(),
            stream: None,
        }
    }

    pub(super) fn streaming(output: &'a mut D, context: &'a Context) -> Self {
        Self {
            writer: Writer::new(),
            stream: Some(Stream { output, context }),
        }
    }

    fn apply<T>(
        &mut self,
        action: impl FnOnce(&mut Writer) -> Result<T, Error>,
    ) -> Result<T, NativeFailure> {
        let result = action(&mut self.writer)?;
        if let Some(stream) = &mut self.stream {
            let fragment = self.writer.pending_output()?;
            if fragment.len() >= crate::input::WORK_BYTES {
                stream
                    .output
                    .text_fragment(fragment, stream.context)
                    .map_err(NativeFailure::Mysql)?;
                self.writer.consume_output()?;
            }
        }
        Ok(result)
    }

    pub(super) fn finish(self) -> Result<String, NativeFailure> {
        let Self { writer, stream } = self;
        if let Some(stream) = stream {
            let tail = writer.finish_tail()?;
            stream
                .output
                .text_fragment(&tail, stream.context)
                .map_err(NativeFailure::Mysql)?;
            Ok(String::new())
        } else {
            writer.finish().map_err(Into::into)
        }
    }

    structural!(array_start);
    structural!(array_end);
    structural!(object_start);
    structural!(object_end);
    structural!(key_start, length: usize);
    structural!(direct_key, key: &DirectKey);
    structural!(integer_start, negative: bool);
    structural!(integer_fragment, digits: &[u8]);
    structural!(integer_end);
}

impl<D: Destination + ?Sized> Consumer for Output<'_, D> {
    type Context = Options;
    type Error = NativeFailure;

    fn write(&mut self, value: ScalarRef<'_>, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.write(value, context))
    }
    fn decimal_start(
        &mut self,
        header: Header,
        context: &Options,
    ) -> Result<Option<Layout>, NativeFailure> {
        self.apply(|writer| writer.decimal_start(header, context))
    }
    fn decimal_fragment(&mut self, ascii: &str, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.decimal_fragment(ascii, context))
    }
    fn decimal_end(&mut self, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.decimal_end(context))
    }
    fn text_start(
        &mut self,
        length: Option<usize>,
        context: &Options,
    ) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.text_start(length, context))
    }
    fn text_fragment(&mut self, text: &str, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.text_fragment(text, context))
    }
    fn text_end(&mut self, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.text_end(context))
    }
    fn bytes_start(&mut self, length: usize, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.bytes_start(length, context))
    }
    fn bytes_fragment(&mut self, bytes: &[u8], context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.bytes_fragment(bytes, context))
    }
    fn bytes_end(&mut self, context: &Options) -> Result<(), NativeFailure> {
        self.apply(|writer| writer.bytes_end(context))
    }
}
