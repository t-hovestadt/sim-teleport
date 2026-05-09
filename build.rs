fn main() {
    let iracing_toml = "deps/iracing-teleport/teleport/Cargo.toml";
    if !std::path::Path::new(iracing_toml).exists() {
        panic!("\n\nSubmodules not initialized.\nRun: git submodule update --init --recursive\n\n");
    }

    for (name, path) in [
        (
            "IRACING_TELEPORT_VERSION",
            "deps/iracing-teleport/teleport/Cargo.toml",
        ),
        ("AC_TELEPORT_VERSION", "crates/ac-teleport/Cargo.toml"),
        ("SIM_RELAY_VERSION", "crates/sim-relay/Cargo.toml"),
    ] {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let version = content
            .lines()
            .find(|l| l.starts_with("version"))
            .and_then(|l| l.split('"').nth(1))
            .unwrap_or("unknown");
        println!("cargo:rustc-env={name}={version}");
    }
}
