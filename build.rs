fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/anarc.ico");
    println!("cargo:rerun-if-changed=assets/anarc.rc");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" {
        // GPUI deeply nested view trees and debug call stacks overflow the default 1MB Windows stack.
        // Increase stack reserve to 16MB (matching Linux capability and Zed GPUI requirements).
        if target_env == "msvc" {
            println!("cargo:rustc-link-arg=/STACK:16777216");
        } else {
            println!("cargo:rustc-link-arg=-Wl,--stack,16777216");
        }

        #[cfg(target_os = "windows")]
        {
            if let Err(e) = embed_resource::compile("assets/anarc.rc", embed_resource::NONE).manifest_optional() {
                eprintln!("cargo:warning=Failed to compile Windows resource: {e}");
            }
        }
    }
}
