/// Configure dynamic Python symbol lookup for macOS extension linking.
fn main() {
    pyo3_build_config::add_extension_module_link_args();
}
