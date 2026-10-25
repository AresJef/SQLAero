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
}
