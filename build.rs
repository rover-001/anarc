fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/anarc.ico");
    println!("cargo:rerun-if-changed=assets/anarc.rc");
    println!("cargo:rerun-if-changed=assets/anarc.manifest.xml");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        #[cfg(target_os = "windows")]
        {
            if let Err(e) = embed_resource::compile("assets/anarc.rc", embed_resource::NONE).manifest_optional() {
                eprintln!("cargo:warning=Failed to compile Windows resource: {e}");
            }
        }
    }
}
