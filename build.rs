fn main() {
    // Man page generation is done via: cargo run --bin generate-man
    // This keeps the build process simple
    println!("cargo:rerun-if-changed=src/cli.rs");
}
