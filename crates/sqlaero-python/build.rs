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
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Python's Windows headers request the interpreter's import library.
        // Reuse PyO3's selected interpreter directory for the C shim as well.
        let directory = config
            .lib_dir()
            .expect("the selected CPython import-library directory is required on Windows");
        println!("cargo:rustc-link-search=native={directory}");
    }
    // Native builds use the same interpreter selected by PyO3/maturin. No
    // runtime interpreter discovery, private Rust object layout or global
    // compiler setting is needed for the narrow integer export shim.
    let includes = config
        .run_python_script(
            "import sysconfig\np = sysconfig.get_paths()\nprint(p['include'])\nprint(p['platinclude'])\n",
        )
        .expect("cannot locate the selected CPython headers for native integer export");
    let mut build = cc::Build::new();
    for include in includes.lines().filter(|line| !line.is_empty()) {
        build.include(include);
    }
    build
        .file("src/input/long_export.c")
        .include("../../third-party/pythoncapi-compat")
        .flag_if_supported("-std=c11")
        .compile("sqlaero_long_export");
    println!("cargo:rerun-if-changed=src/input/long_export.c");
    println!("cargo:rerun-if-changed=../../third-party/pythoncapi-compat/pythoncapi_compat.h");
}
