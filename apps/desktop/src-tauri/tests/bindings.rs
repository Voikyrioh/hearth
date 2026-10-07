//! Les types TypeScript committés doivent refléter les commandes Rust.
//! Régénération : `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`.
#![allow(clippy::unwrap_used, clippy::expect_used)] // tests : les helpers peuvent paniquer

#[path = "../../../../crates/hearth-agent/tests/support/tmp.rs"]
mod tmp;

use hearth_desktop_lib::{BINDINGS_PATH, specta_builder, typescript};

fn normalize(text: String) -> String {
    text.replace("\r\n", "\n")
}

#[test]
fn bindings_file_is_up_to_date() {
    let dir = tmp::tempdir().unwrap();
    let out = dir.path().join("bindings.ts");
    specta_builder().export(typescript(), &out).unwrap();
    let fresh = normalize(std::fs::read_to_string(&out).unwrap());
    if std::env::var_os("HEARTH_REGEN_BINDINGS").is_some() {
        std::fs::write(BINDINGS_PATH, &fresh).unwrap();
    }
    let committed = normalize(std::fs::read_to_string(BINDINGS_PATH).unwrap());
    assert_eq!(
        fresh, committed,
        "src/bindings.ts est périmé : régénère-le avec `HEARTH_REGEN_BINDINGS=1 cargo test -p hearth-desktop`"
    );
}
