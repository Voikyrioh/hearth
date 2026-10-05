//! Tâches de build du dépôt (`cargo xtask <tâche>`).

mod br_check;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help") => {
            println!(
                "tâches : br-check (toute référence BR-… du code et des docs a sa fiche) ; `agent` arrive avec HRT-15"
            );
            ExitCode::SUCCESS
        }
        Some("br-check") => br_check::run(),
        Some(other) => {
            eprintln!("tâche inconnue : {other}");
            ExitCode::FAILURE
        }
    }
}
