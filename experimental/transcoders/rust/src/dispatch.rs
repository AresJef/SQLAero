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
    HotRegistry,
}
/// Owned exact type pointers and callback entries pinned by Python module lifetime.
#[pyclass]
pub struct Dispatch {
    ids: Vec<(usize, Kind)>,
    tags: HashMap<usize, Kind>,
    strategy: Strategy,
}
impl Dispatch {
    /// Cache exact builtin identities once; extension families use the reviewed type catalogue.
    pub fn new(py: Python<'_>) -> Self {
        let entries: [(usize, Kind); 6] = [
            (py.get_type::<PyString>().as_ptr() as usize, Kind::String),
            (py.get_type::<PyInt>().as_ptr() as usize, Kind::Int),
            (py.get_type::<PyBool>().as_ptr() as usize, Kind::Bool),
            (py.None().bind(py).get_type().as_ptr() as usize, Kind::Null),
            (py.get_type::<PyFloat>().as_ptr() as usize, Kind::Float),
            (py.get_type::<PyBytes>().as_ptr() as usize, Kind::Bytes),
        ];
        Self {
            ids: entries.iter().map(|(p, k)| (*p, *k)).collect(),
            tags: entries.iter().map(|(p, k)| (*p, *k)).collect(),
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
        "hot_registry" => Strategy::HotRegistry,
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unknown internal dispatch strategy",
            ))
        }
    };
    Ok(())
}

/// Enumerate meaningful internal classifier candidates, without a selected performance winner.
#[pyfunction]
pub fn _dispatch_strategies() -> (&'static str, &'static str, &'static str) {
    ("cascade", "type_id", "hot_registry")
}
