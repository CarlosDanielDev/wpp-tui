use std::env;

fn main() {
    // The default build uses the pure-Rust MockBackend and needs no Go toolchain.
    // Only compile the Go/whatsmeow c-archive when the `whatsmeow` feature is on.
    if env::var("CARGO_FEATURE_WHATSMEOW").is_err() {
        return;
    }

    // P2 (QR login) wires this up:
    //   go build -buildmode=c-archive -o libwppbridge.a ./bridge
    //   cargo:rustc-link-search / cargo:rustc-link-lib for the archive.
    // Until then the feature is a compiling stub.
    println!("cargo:warning=the `whatsmeow` feature is a stub until the QR-login phase (P2)");
}
