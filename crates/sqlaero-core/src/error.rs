//! Stable client error identities, independent of Python exception objects.

/// Registered client codes live outside MySQL's unsigned 16-bit ERR namespace.
/// New meanings receive a new number; existing numbers are never repurposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ClientErrorCode {
    /// DataError: an accurate numeric value exceeds the approved SQL policy.
    NumericRange = 65_536,
    /// InterfaceError: confirmed session state does not admit this operation.
    SessionState = 65_537,
    /// OperationalError: a configured native request or result limit was hit.
    ResourceLimit = 65_538,
    /// OperationalError: malformed or incomplete server protocol data.
    Protocol = 65_539,
    /// NotSupportedError: a known charset cannot be used by this operation.
    UnsupportedCharset = 65_540,
    /// ProgrammingError: an unsupported active SQL template directive.
    TemplateBinding = 65_541,
    /// ProgrammingError: current adapter width differs from SQL positions.
    ParameterCount = 65_542,
    /// DataError: a true container cycle or excessive active nesting depth.
    ContainerNesting = 65_543,
    /// OperationalTimeoutError: the original whole-operation deadline expired.
    OperationDeadline = 65_544,
    /// DataError: a supported result field contains invalid or unrepresentable data.
    DecodeContent = 65_545,
    /// InterfaceBusyError: another operation owns this connection or Cursor.
    InterfaceBusy = 65_546,
    /// InternalError: a real native ownership/state invariant failed.
    InternalState = 65_547,
    /// OperationalError: actual connection preparation or transport I/O failed.
    Transport = 65_548,
    /// OperationalError: the negotiated TLS channel failed.
    Tls = 65_549,
    /// OperationalError: native authentication could not complete safely.
    Authentication = 65_550,
    /// NotSupportedError: an actual protocol capability is not implemented.
    UnsupportedCapability = 65_551,
    /// OperationalTimeoutError: original connection establishment deadline.
    ConnectDeadline = 65_552,
    /// OperationalTimeoutError: a read made no progress before its deadline.
    ReadDeadline = 65_553,
    /// OperationalTimeoutError: a write made no progress before its deadline.
    WriteDeadline = 65_554,
}
