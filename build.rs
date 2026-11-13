// 仅在 macOS 上编译 Objective-C 定位桥，并链接 CoreLocation / Foundation。
fn main() {
    println!("cargo:rerun-if-changed=src/platform/macos_geo.m");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("src/platform/macos_geo.m")
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .compile("urlbox_geo");
        println!("cargo:rustc-link-lib=framework=CoreLocation");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
