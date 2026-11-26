//! Native interval constructors using the same expression identity as functions.

use super::sql_func::{Kind, SqlFunction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use sqlaero_core::bind::sql_func::Form;
use sqlaero_core::bind::sql_intv;

#[pyclass(subclass, extends = SqlFunction, name = "_SQLInterval", module = "sqlaero._sqlaero")]
pub(crate) struct SqlInterval;

/// Create a MySQL interval measured in days.
#[pyclass(extends = SqlInterval, name = "DAY", module = "sqlaero.sqlintv")]
struct Day;

#[pymethods]
impl Day {
    #[new]
    fn new(days: Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        const FORM: Form = Form::Static(&sql_intv::single(" DAY"));
        let operands = PyTuple::new(days.py(), [days])?;
        let expression = SqlFunction::new("DAY", Kind::Interval, FORM, operands)?;
        Ok(PyClassInitializer::from(expression)
            .add_subclass(SqlInterval)
            .add_subclass(Self))
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Day>()
}
