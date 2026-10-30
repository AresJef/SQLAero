//! Keep interpreter and ABI admission at the Python build boundary.

fn main() {
    pyo3_build_config::use_pyo3_cfgs();
    let config = pyo3_build_config::get();
    let version = config.version();
    assert!(
        config.implementation() == pyo3_build_config::PythonImplementation::CPython
            && version.major == 3
            && (12..=15).contains(&version.minor),
        "SQLAero requires CPython 3.12 through 3.15"
    );
    assert!(
        !config.is_free_threaded() || version.minor >= 14,
        "SQLAero free-threaded builds require CPython 3.14t or 3.15t; 3.13t is not supported"
    );
}
