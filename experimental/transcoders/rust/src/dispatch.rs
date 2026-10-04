//! Interchangeable exact-type classifiers feeding the same SQL value handlers.
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyFloat, PyInt, PyString};
use std::collections::HashMap;
/// Internal primitive handler identifier; trust checks always precede this classification.
#[derive(Clone, Copy)]
pub enum Kind {
    Unknown,
    Null,
    Bool,
    Int,
    Float,
    String,
    Bytes,
}
/// Classifier variants available for controlled future comparison, with no selected speed winner.
#[derive(Clone, Copy)]
enum Strategy {
    Cascade,
    TypeId,
    Table,
    HotRegistry,
}
/// Owned exact type pointers and callback entries pinned by Python module lifetime.
#[pyclass]
pub struct Dispatch {
    ids: Vec<(usize, Kind)>,
    tags: HashMap<usize, Kind>,
    table: HashMap<usize, fn() -> Kind>,
    strategy: Strategy,
}
/// Return the identifier for the shared null handler.
fn null() -> Kind {
    Kind::Null
}
/// Return the identifier for the shared boolean handler.
fn boolean() -> Kind {
    Kind::Bool
}
/// Return the identifier for the shared integer handler.
fn integer() -> Kind {
    Kind::Int
}
/// Return the identifier for the shared floating handler.
fn floating() -> Kind {
    Kind::Float
}
/// Return the identifier for the shared Unicode handler.
fn string() -> Kind {
    Kind::String
}
/// Return the identifier for the shared binary handler.
fn bytes() -> Kind {
    Kind::Bytes
}
impl Dispatch {
    /// Cache exact builtin identities once; extension families use the reviewed type catalogue.
    pub fn new(py: Python<'_>) -> Self {
        let entries: [(usize, Kind, fn() -> Kind); 6] = [
            (
                py.get_type::<PyString>().as_ptr() as usize,
                Kind::String,
                string,
            ),
            (py.get_type::<PyInt>().as_ptr() as usize, Kind::Int, integer),
            (
                py.get_type::<PyBool>().as_ptr() as usize,
                Kind::Bool,
                boolean,
            ),
            (
                py.None().bind(py).get_type().as_ptr() as usize,
                Kind::Null,
                null,
            ),
            (
                py.get_type::<PyFloat>().as_ptr() as usize,
                Kind::Float,
                floating,
            ),
            (
                py.get_type::<PyBytes>().as_ptr() as usize,
                Kind::Bytes,
                bytes,
            ),
        ];
        Self {
            ids: entries.iter().map(|(p, k, _)| (*p, *k)).collect(),
            tags: entries.iter().map(|(p, k, _)| (*p, *k)).collect(),
            table: entries.iter().map(|(p, _, f)| (*p, *f)).collect(),
            strategy: Strategy::HotRegistry,
        }
    }
    /// Classify exactly once, counting pointer acquisition and tag lookup as dispatch work.
    fn classify(&self, o: &Bound<'_, PyAny>) -> Kind {
        let ptr = o.get_type().as_ptr() as usize;
        match self.strategy {
            Strategy::Cascade => {
                for (p, k) in &self.ids {
                    if *p == ptr {
                        return *k;
                    }
                }
                Kind::Unknown
            }
            Strategy::TypeId => self.tags.get(&ptr).copied().unwrap_or(Kind::Unknown),
            Strategy::Table => self.table.get(&ptr).map(|f| f()).unwrap_or(Kind::Unknown),
            Strategy::HotRegistry => {
                for (p, k) in &self.ids[..5] {
                    if *p == ptr {
                        return *k;
                    }
                }
                self.tags.get(&ptr).copied().unwrap_or(Kind::Unknown)
            }
        }
    }
}
/// Feed one of the internal classifier candidates into identical serialization handlers.
pub fn classify(o: &Bound<'_, PyAny>) -> PyResult<Kind> {
    let state = o
        .py()
        .import("sqlaero_rust_kernel")?
        .getattr("_native_dispatch")?;
    let state: PyRef<'_, Dispatch> = state.extract().map_err(PyErr::from)?;
    Ok(state.classify(o))
}
/// Select an internal benchmark candidate; this is not a public custom-type registration API.
#[pyfunction]
pub fn _set_dispatch_strategy(py: Python<'_>, strategy: &str) -> PyResult<()> {
    let state = py
        .import("sqlaero_rust_kernel")?
        .getattr("_native_dispatch")?;
    let mut state: PyRefMut<'_, Dispatch> = state.extract().map_err(PyErr::from)?;
    state.strategy = match strategy {
        "cascade" => Strategy::Cascade,
        "type_id" => Strategy::TypeId,
        "function_table" => Strategy::Table,
        "hot_registry" => Strategy::HotRegistry,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown internal dispatch strategy",
            ))
        }
    };
    Ok(())
}
