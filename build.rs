// build.rs — compile the Objective-C UI shell (app_darwin.m) and link Cocoa.
//
// The .m file is a peer to the Rust crate: it implements the C entry points
// we declare as `extern "C"` in src/ffi.rs, and declares `extern` prototypes
// for the Rust callbacks we expose via `#[no_mangle]` in src/main.rs.
fn main() {
    // cc compiles .m as Objective-C when given -x objective-c. -fobjc-arc
    // enables automatic reference counting.
    cc::Build::new()
        .file("app_darwin.m")
        .flag("-x")
        .flag("objective-c")
        .flag("-fobjc-arc")
        .compile("ocg_ui");

    println!("cargo:rustc-link-lib=framework=Cocoa");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rerun-if-changed=app_darwin.m");
}
