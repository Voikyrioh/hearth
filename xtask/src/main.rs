//! Tâches de build du dépôt (`cargo xtask <tâche>`).
//!
//! - `agent` : construit le binaire de l'agent, statique (`x86_64-unknown-linux-musl`), en
//!   conteneur, et le dépose dans `target/dist/hearth-agent`.
//! - `e2e-install` : installe ce binaire dans un conteneur jetable et vérifie tout le parcours
//!   (installation, réinstallation, désinstallation).
//!
//! - `e2e-update` : la mise à jour de l'agent à distance (réussie, retour automatique, une seule à la
//!   fois, signature invalide) sur une machine jetable avec systemd.
//! - `shellcheck` : `deploy/install.sh` et le scénario de bout en bout passent `shellcheck`.
//! - `br-check` : toute référence `BR-…` du code et des docs a sa fiche.
//! - `client-release-check`, `client-manifest` : publication du client (HRT-16, runbook
//!   `publier-une-version-du-client`).
//!
//! Aucune commande n'est lancée par un interpréteur avec une chaîne construite : les programmes
//! reçoivent des listes d'arguments ; les rares scripts `sh -c` sont des constantes, exécutées
//! dans le conteneur, qui reçoivent leurs valeurs par variables d'environnement.

mod agent;
mod br_check;
mod client_release;
mod docker;
mod e2e;
mod e2e_update;
mod shellcheck;

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        None | Some("help") => {
            println!(
                "tâches :\n  agent         construit le binaire statique de l'agent en conteneur (target/dist/hearth-agent)\n  e2e-install   installation de bout en bout dans un conteneur jetable\n  e2e-update    mise à jour de l'agent à distance de bout en bout (retour automatique compris)
  shellcheck    contrôle les scripts de deploy/ avec shellcheck (en conteneur)\n  br-check      toute référence BR-… du code et des docs a sa fiche
  client-release-check  refuse la clé de développement et une version qui n'est pas celle du dépôt
  client-manifest       écrit latest.json (manifeste du greffon de mise à jour du client)"
            );
            return ExitCode::SUCCESS;
        }
        Some("agent") => agent::run().map(|_| ()),
        Some("e2e-install") => e2e::run(),
        Some("e2e-update") => e2e_update::run(),
        Some("shellcheck") => shellcheck::run(),
        Some("br-check") => return br_check::run(),
        Some("client-release-check") => {
            client_release::run_check(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("client-manifest") => {
            client_release::run_manifest(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some(other) => Err(format!("tâche inconnue : {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("erreur : {message}");
            ExitCode::FAILURE
        }
    }
}
