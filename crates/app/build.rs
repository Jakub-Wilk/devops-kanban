//! Embeds `icons/icon.ico` and the "Board Buddy" name in the Windows executable, so Explorer,
//! taskbar pins, the Start menu and Task Manager show them. Other targets need nothing: the window
//! icon is set at runtime (`src/app_icon.rs`).

fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("icons/icon.ico")
        .set("ProductName", "Board Buddy")
        .set("FileDescription", "Board Buddy");
    // A missing resource compiler (e.g. cross-compiling without llvm-rc) shouldn't fail the build.
    if let Err(e) = res.compile() {
        println!("cargo:warning=building without the .exe icon: {e}");
    }
}
