fn main() {
    // Expose the project version as AIOS_VERSION at compile time.
    // Reads ../../VERSION (relative to aios-core/) — the canonical version file.
    let version = std::fs::read_to_string("../../VERSION")
        .unwrap_or_else(|_| "0.0.0".to_string());
    let version = version.trim();
    println!("cargo:rustc-env=AIOS_VERSION={version}");
    // Re-run if the VERSION file changes.
    println!("cargo:rerun-if-changed=../../VERSION");
}
