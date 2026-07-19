fn main() {
    // `sqlx::migrate!` embeds the migration files into the binary at compile
    // time. Without this, adding or editing a .sql file does not invalidate
    // the build cache, and the server starts up cheerfully applying the
    // migration set it was last compiled with -- which looks exactly like
    // success while doing nothing.
    println!("cargo:rerun-if-changed=../../migrations");
}
