//! Replication and topology function signatures. The server owns all effects.

use super::calls::{fixed_calls, optional_calls};
use super::{Kind, SqlFunction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::Form;

fixed_calls! {
    FailoverAddSource, "asynchronous_connection_failover_add_source",
        (channel, host, port, network_namespace, weight),
        "Add one source to a replication channel's asynchronous source list.";
    FailoverDeleteManaged, "asynchronous_connection_failover_delete_managed", (channel, managed_name),
        "Remove a managed group from a replication channel's asynchronous source list.";
    FailoverDeleteSource, "asynchronous_connection_failover_delete_source", (channel, host, port, network_namespace),
        "Remove a source from a replication channel's asynchronous source list.";
    FailoverReset, "asynchronous_connection_failover_reset", (),
        "Clear asynchronous failover settings on a server outside a replication group.";
    DisableMemberAction, "group_replication_disable_member_action", (name, event),
        "Disable a member action for the specified group replication event.";
    EnableMemberAction, "group_replication_enable_member_action", (name, event),
        "Enable a member action for the specified group replication event.";
    GetCommunicationProtocol, "group_replication_get_communication_protocol", (),
        "Return the communication protocol version used by the replication group.";
    GetWriteConcurrency, "group_replication_get_write_concurrency", (),
        "Return the group replication maximum number of consensus instances in progress.";
    ResetMemberActions, "group_replication_reset_member_actions", (),
        "Restore default member actions on a server outside a replication group.";
    SetCommunicationProtocol, "group_replication_set_communication_protocol", (version),
        "Set the replication group's communication protocol to the supplied server version.";
    SetWriteConcurrency, "group_replication_set_write_concurrency", (concurrency),
        "Set the group replication maximum number of consensus instances in progress.";
    SwitchToMultiPrimary, "group_replication_switch_to_multi_primary_mode", (),
        "Switch a running replication group into multi-primary mode.";
}

optional_calls! {
    SetAsPrimary, "group_replication_set_as_primary", (member_uuid), (timeout),
        "(member_uuid, timeout=...)", "Appoint a replication group member as primary, optionally with a transaction timeout.";
    SwitchToSinglePrimary, "group_replication_switch_to_single_primary_mode", (), (member_uuid),
        "(member_uuid=...)", "Switch to single-primary mode, optionally selecting a member by UUID.";
    MasterPosWait, "MASTER_POS_WAIT", (log_name, log_pos), (timeout, channel),
        "(log_name, log_pos, timeout=..., channel=...)", "Wait for a replica to reach a source binlog position using the deprecated name.";
    SourcePosWait, "SOURCE_POS_WAIT", (log_name, log_pos), (timeout, channel),
        "(log_name, log_pos, timeout=..., channel=...)", "Wait for a replica to reach a source binlog position.";
    WaitForExecutedGtidSet, "WAIT_FOR_EXECUTED_GTID_SET", (gtid_set), (timeout),
        "(gtid_set, timeout=...)", "Wait until the local server has executed the given GTID set.";
}

/// Add a managed group to a replication channel's asynchronous source list.
#[pyclass(
    extends = SqlFunction,
    name = "asynchronous_connection_failover_add_managed",
    module = "sqlaero.sqlfunc"
)]
struct FailoverAddManaged;

#[pymethods]
impl FailoverAddManaged {
    #[new]
    #[allow(
        clippy::too_many_arguments,
        reason = "The public signature mirrors all eight named MySQL arguments."
    )]
    fn new<'py>(
        channel: Bound<'py, PyAny>,
        managed_type: Bound<'py, PyAny>,
        managed_name: Bound<'py, PyAny>,
        host: Bound<'py, PyAny>,
        port: Bound<'py, PyAny>,
        network_namespace: Bound<'py, PyAny>,
        primary_weight: Bound<'py, PyAny>,
        secondary_weight: Bound<'py, PyAny>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let operands = PyTuple::new(
            channel.py(),
            [
                channel,
                managed_type,
                managed_name,
                host,
                port,
                network_namespace,
                primary_weight,
                secondary_weight,
            ],
        )?;
        let expression = SqlFunction::new(
            "asynchronous_connection_failover_add_managed",
            Kind::Scalar,
            Form::Call("asynchronous_connection_failover_add_managed("),
            operands,
        )?;
        Ok(PyClassInitializer::from(expression).add_subclass(Self))
    }
}

pub(super) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    register_fixed(module)?;
    register_optional(module)?;
    module.add_class::<FailoverAddManaged>()
}
