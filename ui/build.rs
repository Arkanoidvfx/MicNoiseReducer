fn main() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = root.join("assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(icon.to_str().unwrap())
            .set("ProductName", "Mic Noize")
            .set("FileDescription", "Mic Noize")
            .compile()
            .expect("embed application icon");
    }
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("../build/native/Release").display()
    );
    println!("cargo:rustc-link-lib=static=mic_engine");
    println!("cargo:rustc-link-lib=static=rubberband");
    for lib in [
        "ole32", "oleaut32", "taskschd", "advapi32", "uuid", "avrt", "user32", "shell32", "wtsapi32", "gdi32", "mmdevapi", "setupapi",
    ] {
        println!("cargo:rustc-link-lib={lib}");
    }
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=../build/native/Release/mic_engine.lib");
    println!("cargo:rerun-if-changed=../build/native/Release/rubberband.lib");
}
