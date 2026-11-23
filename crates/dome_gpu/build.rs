fn main() {
    for backend in ["metal", "vulkan"] {
        println!("cargo::rustc-check-cfg=cfg({backend}_backend)");
    }

    let os = std::env::var("CARGO_CFG_TARGET_OS").expect("cargo names the target OS");
    let backend = match os.as_str() {
        "windows" | "linux" | "android" => "vulkan",
        "macos" | "ios" => "metal",
        unsupported => panic!("no gpu backend targets {unsupported}"),
    };
    println!("cargo::rustc-cfg={backend}_backend");
}
