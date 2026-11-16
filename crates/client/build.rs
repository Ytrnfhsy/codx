//! On Windows the game's icon goes into the executable itself, so Explorer, the desktop
//! and the task bar show it before the game even starts.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/icon/icon.ico");
    res.set("ProductName", "Toon Warfare");
    res.set("FileDescription", "Toon Warfare");
    // (without a resource compiler the game still builds, just without its icon)
    if let Err(e) = res.compile() {
        println!("cargo:warning=no icon in the executable: {e}");
    }
}
