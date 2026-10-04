//! Tâches de build du dépôt (`cargo xtask <tâche>`).

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help") => {
            println!("tâches : (aucune pour l'instant, `agent` arrive avec HRT-15)");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("tâche inconnue : {other}");
            ExitCode::FAILURE
        }
    }
}
