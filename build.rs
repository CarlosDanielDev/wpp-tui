use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // The default build uses the pure-Rust MockBackend and needs no Go toolchain.
    // Only compile the Go/whatsmeow c-archive when the `whatsmeow` feature is on.
    if env::var("CARGO_FEATURE_WHATSMEOW").is_err() {
        return;
    }

    // Rebuild the archive whenever the Go side changes.
    println!("cargo:rerun-if-changed=bridge/bridge.go");
    println!("cargo:rerun-if-changed=bridge/go.mod");
    println!("cargo:rerun-if-changed=bridge/go.sum");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let archive = out_dir.join("libwppbridge.a");

    // `bridge/` is its own Go module, so run the build from inside it and emit
    // the archive to an absolute path in OUT_DIR:
    //   go build -buildmode=c-archive -o $OUT_DIR/libwppbridge.a .
    let status = Command::new("go")
        .current_dir(manifest_dir.join("bridge"))
        .args([
            "build",
            "-buildmode=c-archive",
            "-o",
            archive.to_str().expect("OUT_DIR path is not valid UTF-8"),
            ".",
        ])
        .status()
        .expect("failed to spawn `go` — a Go toolchain is required for the `whatsmeow` feature");

    if !status.success() {
        panic!("`go build -buildmode=c-archive` failed (exit {status})");
    }

    // Link the static archive produced above.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=wppbridge");

    // The Go runtime's c-archive needs a few platform libraries at link time.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=framework=Security");
            println!("cargo:rustc-link-lib=resolv");
        }
        "linux" => {
            println!("cargo:rustc-link-lib=pthread");
            println!("cargo:rustc-link-lib=dl");
        }
        _ => {}
    }
}
