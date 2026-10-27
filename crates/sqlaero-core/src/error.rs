//! Generated from data/error-catalog.json (SHA-256 670d0d05aa7a96d1a5368a2e1fb2b1880c9b821ab5fd9ca18acdf47ec0108dcb); edit the registry and run tools/error_catalog.py.

/// Exact native exception categories used by the maintained error registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionClass {
    /// The public InterfaceError category.
    Interface,
    /// The public InterfaceBusyError category.
    InterfaceBusy,
    /// The public DatabaseError category.
    Database,
    /// The public DataError category.
    Data,
    /// The public OperationalError category.
    Operational,
    /// The public OperationalTimeoutError category.
    OperationalTimeout,
    /// The public IntegrityError category.
    Integrity,
    /// The public InternalError category.
    Internal,
    /// The public ProgrammingError category.
    Programming,
    /// The public NotSupportedError category.
    NotSupported,
}

/// Registered client codes outside MySQL unsigned 16-bit ERR codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ClientErrorCode {
    /// DataError: an accurate numeric value exceeds the approved SQL policy.
    NumericRange = 65536,
    /// InterfaceError: confirmed session state does not admit this operation.
    SessionState = 65537,
    /// OperationalError: a configured native request or result limit was hit.
    ResourceLimit = 65538,
    /// OperationalError: malformed or incomplete server protocol data.
    Protocol = 65539,
    /// NotSupportedError: a known charset cannot be used by this operation.
    UnsupportedCharset = 65540,
    /// ProgrammingError: an unsupported active SQL template directive.
    TemplateBinding = 65541,
    /// ProgrammingError: current adapter width differs from SQL positions.
    ParameterCount = 65542,
    /// DataError: a true container cycle or excessive active nesting depth.
    ContainerNesting = 65543,
    /// OperationalTimeoutError: the original whole-operation deadline expired.
    OperationDeadline = 65544,
    /// DataError: a supported result field contains invalid or unrepresentable data.
    DecodeContent = 65545,
    /// InterfaceBusyError: another operation owns this connection or Cursor.
    InterfaceBusy = 65546,
    /// InternalError: a real native ownership/state invariant failed.
    InternalState = 65547,
    /// OperationalError: actual connection preparation or transport I/O failed.
    Transport = 65548,
    /// OperationalError: the negotiated TLS channel failed.
    Tls = 65549,
    /// OperationalError: native authentication could not complete safely.
    Authentication = 65550,
    /// NotSupportedError: an actual protocol capability is not implemented.
    UnsupportedCapability = 65551,
    /// OperationalTimeoutError: original connection establishment deadline.
    ConnectDeadline = 65552,
    /// OperationalTimeoutError: a read made no progress before its deadline.
    ReadDeadline = 65553,
    /// OperationalTimeoutError: a write made no progress before its deadline.
    WriteDeadline = 65554,
}

/// One maintained client identity and its precise public behavior.
pub struct ClientEntry {
    /// Stable code and native identity.
    pub code: ClientErrorCode,
    /// Public IntEnum member name.
    pub name: &'static str,
    /// Exact exception category.
    pub class: ExceptionClass,
    /// Complete English member documentation.
    pub doc: &'static str,
}
/// All registered client errors in stable numeric order.
pub static CLIENT_CODES: &[ClientEntry] = &[
    ClientEntry {
        code: ClientErrorCode::NumericRange,
        name: "NUMERIC_RANGE",
        class: ExceptionClass::Data,
        doc: "An accurate numeric value exceeds the approved sql policy. Code 65536; client origin (is_server_error=False); exact exception class DataError.",
    },
    ClientEntry {
        code: ClientErrorCode::SessionState,
        name: "SESSION_STATE",
        class: ExceptionClass::Interface,
        doc: "Confirmed session state does not admit this operation. Code 65537; client origin (is_server_error=False); exact exception class InterfaceError.",
    },
    ClientEntry {
        code: ClientErrorCode::ResourceLimit,
        name: "RESOURCE_LIMIT",
        class: ExceptionClass::Operational,
        doc: "A configured native request or result limit was hit. Code 65538; client origin (is_server_error=False); exact exception class OperationalError.",
    },
    ClientEntry {
        code: ClientErrorCode::Protocol,
        name: "PROTOCOL",
        class: ExceptionClass::Operational,
        doc: "Malformed or incomplete server protocol data. Code 65539; client origin (is_server_error=False); exact exception class OperationalError.",
    },
    ClientEntry {
        code: ClientErrorCode::UnsupportedCharset,
        name: "UNSUPPORTED_CHARSET",
        class: ExceptionClass::NotSupported,
        doc: "A known charset cannot be used by this operation. Code 65540; client origin (is_server_error=False); exact exception class NotSupportedError.",
    },
    ClientEntry {
        code: ClientErrorCode::TemplateBinding,
        name: "TEMPLATE_BINDING",
        class: ExceptionClass::Programming,
        doc: "An unsupported active sql template directive. Code 65541; client origin (is_server_error=False); exact exception class ProgrammingError.",
    },
    ClientEntry {
        code: ClientErrorCode::ParameterCount,
        name: "PARAMETER_COUNT",
        class: ExceptionClass::Programming,
        doc: "Current adapter width differs from sql positions. Code 65542; client origin (is_server_error=False); exact exception class ProgrammingError.",
    },
    ClientEntry {
        code: ClientErrorCode::ContainerNesting,
        name: "CONTAINER_NESTING",
        class: ExceptionClass::Data,
        doc: "A true container cycle or excessive active nesting depth. Code 65543; client origin (is_server_error=False); exact exception class DataError.",
    },
    ClientEntry {
        code: ClientErrorCode::OperationDeadline,
        name: "OPERATION_DEADLINE",
        class: ExceptionClass::OperationalTimeout,
        doc: "The original whole-operation deadline expired. Code 65544; client origin (is_server_error=False); exact exception class OperationalTimeoutError.",
    },
    ClientEntry {
        code: ClientErrorCode::DecodeContent,
        name: "DECODE_CONTENT",
        class: ExceptionClass::Data,
        doc: "A supported result field contains invalid or unrepresentable data. Code 65545; client origin (is_server_error=False); exact exception class DataError.",
    },
    ClientEntry {
        code: ClientErrorCode::InterfaceBusy,
        name: "INTERFACE_BUSY",
        class: ExceptionClass::InterfaceBusy,
        doc: "Another operation owns this connection or cursor. Code 65546; client origin (is_server_error=False); exact exception class InterfaceBusyError.",
    },
    ClientEntry {
        code: ClientErrorCode::InternalState,
        name: "INTERNAL_STATE",
        class: ExceptionClass::Internal,
        doc: "A real native ownership/state invariant failed. Code 65547; client origin (is_server_error=False); exact exception class InternalError.",
    },
    ClientEntry {
        code: ClientErrorCode::Transport,
        name: "TRANSPORT",
        class: ExceptionClass::Operational,
        doc: "Actual connection preparation or transport i/o failed. Code 65548; client origin (is_server_error=False); exact exception class OperationalError.",
    },
    ClientEntry {
        code: ClientErrorCode::Tls,
        name: "TLS",
        class: ExceptionClass::Operational,
        doc: "The negotiated tls channel failed. Code 65549; client origin (is_server_error=False); exact exception class OperationalError.",
    },
    ClientEntry {
        code: ClientErrorCode::Authentication,
        name: "AUTHENTICATION",
        class: ExceptionClass::Operational,
        doc: "Native authentication could not complete safely. Code 65550; client origin (is_server_error=False); exact exception class OperationalError.",
    },
    ClientEntry {
        code: ClientErrorCode::UnsupportedCapability,
        name: "UNSUPPORTED_CAPABILITY",
        class: ExceptionClass::NotSupported,
        doc: "An actual protocol capability is not implemented. Code 65551; client origin (is_server_error=False); exact exception class NotSupportedError.",
    },
    ClientEntry {
        code: ClientErrorCode::ConnectDeadline,
        name: "CONNECT_DEADLINE",
        class: ExceptionClass::OperationalTimeout,
        doc: "Original connection establishment deadline. Code 65552; client origin (is_server_error=False); exact exception class OperationalTimeoutError.",
    },
    ClientEntry {
        code: ClientErrorCode::ReadDeadline,
        name: "READ_DEADLINE",
        class: ExceptionClass::OperationalTimeout,
        doc: "A read made no progress before its deadline. Code 65553; client origin (is_server_error=False); exact exception class OperationalTimeoutError.",
    },
    ClientEntry {
        code: ClientErrorCode::WriteDeadline,
        name: "WRITE_DEADLINE",
        class: ExceptionClass::OperationalTimeout,
        doc: "A write made no progress before its deadline. Code 65554; client origin (is_server_error=False); exact exception class OperationalTimeoutError.",
    },
];
impl ClientErrorCode {
    /// The one registered source for this client code's class and public name.
    #[must_use]
    pub fn entry(self) -> &'static ClientEntry {
        match self {
            Self::NumericRange => &CLIENT_CODES[0],
            Self::SessionState => &CLIENT_CODES[1],
            Self::ResourceLimit => &CLIENT_CODES[2],
            Self::Protocol => &CLIENT_CODES[3],
            Self::UnsupportedCharset => &CLIENT_CODES[4],
            Self::TemplateBinding => &CLIENT_CODES[5],
            Self::ParameterCount => &CLIENT_CODES[6],
            Self::ContainerNesting => &CLIENT_CODES[7],
            Self::OperationDeadline => &CLIENT_CODES[8],
            Self::DecodeContent => &CLIENT_CODES[9],
            Self::InterfaceBusy => &CLIENT_CODES[10],
            Self::InternalState => &CLIENT_CODES[11],
            Self::Transport => &CLIENT_CODES[12],
            Self::Tls => &CLIENT_CODES[13],
            Self::Authentication => &CLIENT_CODES[14],
            Self::UnsupportedCapability => &CLIENT_CODES[15],
            Self::ConnectDeadline => &CLIENT_CODES[16],
            Self::ReadDeadline => &CLIENT_CODES[17],
            Self::WriteDeadline => &CLIENT_CODES[18],
        }
    }
}

pub mod catalog;

#[cfg(test)]
mod tests;
