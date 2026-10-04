// Recompile quand une migration change : `sqlx::migrate!` les embarque à la compilation.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
