//! Tâches de build du dépôt (`cargo xtask <tâche>`).
//!
//! - `agent` : construit le binaire de l'agent, statique (`x86_64-unknown-linux-musl`), en
//!   conteneur, et le dépose dans `target/dist/hearth-agent`.
//! - `e2e-install` : installe ce binaire dans un conteneur jetable et vérifie tout le parcours
//!   (installation, réinstallation, désinstallation).
//!
//! - `e2e-update` : la mise à jour de l'agent à distance (réussie, retour automatique, une seule à la
//!   fois, signature invalide) sur une machine jetable avec systemd.
//! - `e2e-double-failure` : la double panne de la mise à jour de l'agent (HRT-27) sur le systemd de
//!   la machine jetable, avec le binaire construit par `agent`.
//! - `shellcheck` : `deploy/install.sh` et le scénario de bout en bout passent `shellcheck`.
//! - `test-tmp-check` : la suite de tests n'a laissé aucun dossier temporaire derrière elle (HRT-18,
//!   T43).
//! - `br-check` : toute référence `BR-…` du code et des docs a sa fiche.
//! - `client-release-check`, `client-manifest` : publication du client (HRT-16, runbook
//!   `publier-une-version-du-client`).
//! - `agent-manifest` : section `agent` du `latest.json`, signature vérifiée contre la clé
//!   embarquée dans l'agent (HRT-17, ADR-0021, runbook `mettre-a-jour-agent`).
//!
//! Aucune commande n'est lancée par un interpréteur avec une chaîne construite : les programmes
//! reçoivent des listes d'arguments ; les rares scripts `sh -c` sont des constantes, exécutées
//! dans le conteneur, qui reçoivent leurs valeurs par variables d'environnement.

mod agent;
mod agent_release;
mod br_check;
mod client_release;
mod docker;
mod e2e;
mod e2e_double_failure;
mod e2e_update;
mod release_core;
mod shellcheck;
mod test_tmp;

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        None | Some("help") => {
            println!(
                "tâches :\n  agent         construit le binaire statique de l'agent en conteneur (target/dist/hearth-agent)\n  e2e-install   installation de bout en bout dans un conteneur jetable\n  e2e-update    mise à jour de l'agent à distance de bout en bout (retour automatique compris)
  e2e-double-failure  la double panne de la mise à jour de l'agent (superviseur tué, retour arrière sans geste) sur un vrai systemd
  shellcheck    contrôle les scripts de deploy/ avec shellcheck (en conteneur)\n  test-tmp-check  échoue si la suite de tests a laissé des dossiers temporaires (--clean les supprime)\n  br-check      toute référence BR-… du code et des docs a sa fiche
  client-release-check  refuse la clé de développement et une version qui n'est pas celle du dépôt
  client-version        la version du dépôt (seule source du numéro publié)\n  client-sign           signe l'installateur avec la clé des variables d'environnement (version dans la signature)\n  client-manifest       vérifie la signature puis écrit latest.json (manifeste du greffon de mise à jour du client)
  agent-manifest        vérifie la signature de l'agent contre sa clé embarquée puis ajoute la section `agent` à latest.json (cible de la mise à jour de l'agent)"
            );
            return ExitCode::SUCCESS;
        }
        Some("agent") => agent::run().map(|_| ()),
        Some("e2e-install") => e2e::run(),
        Some("e2e-update") => e2e_update::run(),
        Some("e2e-double-failure") => e2e_double_failure::run(),
        Some("shellcheck") => shellcheck::run(),
        Some("test-tmp-check") => test_tmp::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("br-check") => return br_check::run(),
        Some("client-release-check") => {
            client_release::run_check(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("client-version") => client_release::run_version(),
        Some("client-sign") => {
            client_release::run_sign(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("client-manifest") => {
            client_release::run_manifest(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("agent-manifest") => {
            agent_release::run_manifest(&std::env::args().skip(2).collect::<Vec<_>>())
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
