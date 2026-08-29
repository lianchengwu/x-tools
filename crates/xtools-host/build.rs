use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ui_path = manifest_dir.join("../xtools-ui/ui").canonicalize().unwrap();

    let config = slint_build::CompilerConfiguration::new().with_library_paths(
        [("xtools-ui".into(), ui_path)]
            .into_iter()
            .collect(),
    );
    slint_build::compile_with_config("ui/runner.slint", config).unwrap();
}
