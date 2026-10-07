fn main() {
    println!("cargo:rerun-if-changed=native/zipvoice-phonemizer");
    if std::env::var_os("CARGO_FEATURE_ZIPVOICE").is_none() {
        return;
    }
    let target = std::env::var("TARGET").expect("Cargo target");
    let mut config = cmake::Config::new("native/zipvoice-phonemizer");
    config
        .profile("Release")
        .build_target("zipvoice-phonemizer");
    if target.contains("windows") {
        config.generator("Ninja");
        if let Some(compiler) = cc::windows_registry::find_tool(&target, "cl.exe") {
            for (key, value) in compiler.env() {
                config.env(key, value);
            }
        }
    }
    let output = config.build();
    let filename = if target.contains("windows") {
        "zipvoice-phonemizer.exe"
    } else {
        "zipvoice-phonemizer"
    };
    std::fs::copy(
        output.join("build").join(filename),
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("build output"))
            .join("zipvoice-phonemizer.bin"),
    )
    .expect("copy the built phonemizer");
}
