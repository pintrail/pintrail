fn main() {
    // SQLx integration tests embed the schema, so refresh them after migrations change.
    println!("cargo:rerun-if-changed=../../migrations");
}
