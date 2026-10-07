use std::{
    env,
    path::{Path, PathBuf},
};

fn link_directories(path: &Path) {
    let mut library_directory = false;
    for entry in std::fs::read_dir(path).expect("CMake output directory") {
        let entry = entry.expect("CMake output entry");
        if entry.file_type().expect("CMake output type").is_dir() {
            link_directories(&entry.path());
        } else if entry
            .path()
            .extension()
            .is_some_and(|ext| ext == "lib" || ext == "a")
        {
            library_directory = true;
        }
    }
    if library_directory {
        println!("cargo:rustc-link-search=native={}", path.display());
    }
}
fn main() {
    println!("cargo:rerun-if-changed=native");
    for name in ["CUDA_PATH", "CUDA_COMPUTE_CAP", "CMAKE_GENERATOR"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let target = env::var("TARGET").expect("Cargo target");
    let windows = target.contains("windows");
    let apple = target.contains("apple");
    let cuda = env::var_os("CARGO_FEATURE_CUDA").is_some() && (windows || target.contains("linux"));
    let metal = env::var_os("CARGO_FEATURE_METAL").is_some() && apple;
    let mut config = cmake::Config::new("native");
    config
        .profile("Release")
        .build_target("trnovel_voxcpm")
        .define("GGML_CUDA", if cuda { "ON" } else { "OFF" })
        .define("GGML_METAL", if metal { "ON" } else { "OFF" });
    if windows {
        // Ninja does not require the optional Visual Studio CUDA project integration.
        config.generator("Ninja");
        if let Some(compiler) = cc::windows_registry::find_tool(&target, "cl.exe") {
            for (key, value) in compiler.env() {
                config.env(key, value);
            }
        }
    }
    if cuda {
        config.define(
            "CMAKE_CUDA_ARCHITECTURES",
            env::var("CUDA_COMPUTE_CAP").unwrap_or_else(|_| "native".into()),
        );
        if let Some(root) = env::var_os("CUDA_PATH") {
            config.define("CUDAToolkit_ROOT", root);
        }
    }
    let output = config.build();
    link_directories(&output);
    if windows {
        println!("cargo:rustc-link-lib=advapi32");
    }
    for library in [
        "trnovel_voxcpm",
        "llama-common",
        "llama-common-base",
        "cpp-httplib",
        "llama",
        "ggml",
        "ggml-cpu",
        "ggml-base",
    ] {
        println!("cargo:rustc-link-lib=static={library}");
    }
    if cuda {
        println!("cargo:rustc-link-lib=static=ggml-cuda");
        if let Some(root) = env::var_os("CUDA_PATH") {
            let root = PathBuf::from(root);
            println!(
                "cargo:rustc-link-search=native={}",
                root.join(if windows { "lib/x64" } else { "lib64" })
                    .display()
            );
        }
        for library in ["cudart", "cublas", "cublasLt", "cuda"] {
            println!("cargo:rustc-link-lib={library}");
        }
    }
    if !windows {
        println!(
            "cargo:rustc-link-lib={}",
            if apple { "c++" } else { "stdc++" }
        );
        if apple {
            println!("cargo:rustc-link-lib=framework=Accelerate");
            if metal {
                println!("cargo:rustc-link-lib=static=ggml-metal");
                for framework in ["Foundation", "Metal", "MetalKit"] {
                    println!("cargo:rustc-link-lib=framework={framework}");
                }
            }
        } else {
            for library in ["gomp", "pthread", "dl", "m"] {
                println!("cargo:rustc-link-lib={library}");
            }
        }
    }
}
