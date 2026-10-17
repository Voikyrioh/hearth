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
//! - `catalog-check` : les fiches de serveur de jeu embarquées passent le chargeur réel et `ids.lock` (HRT-113) ;
//!   `--release` (option, aucune publication ne l'appelle) refuse toute fiche `preview`. Lancé par la CI (job `rust`).
//! - `client-release-check`, `client-manifest` : publication du client (HRT-16, runbook
//!   `publier-une-version-du-client`). Les adresses publiées sont celles de
//!   `hearth.voikyrioh.fr/api/releases/` (HRT-49, ADR-0035) ; `client-release-check` refuse un numéro
//!   dont l'étiquette `vX.Y.Z` existe déjà sur `origin`.
//! - `release-preflight` : refuse avant toute construction une version invalide ou déjà publiée
//!   (HRT-172, BR-RELEASE-004, 006 et 008) ; `--versions-only` tourne à chaque PR.
//! - `release-stage`, `release-upload` : assemble une version en local (`target/release-stage/`, tout
//!   revérifié) puis la dépose au bucket (essai à blanc par défaut, `--confirm`, manifeste EN DERNIER ;
//!   HRT-51, BR-RELEASE-001 à 003).
//! - `release-github`, `client-manifest` (notes de `HEARTH_NOTES`, date UTC), messages exacts du dépôt (refus,
//!   dépôt interrompu, essai à blanc) et `aws --version` : signature, assemblage et dépôt relançables sans
//!   danger (HRT-174, BR-RELEASE-009 à 011).
//! - `agent --release`, `client-record`, `agent-sign`, `ci-root-check` : constructions vérifiables du flux
//!   de publication (HRT-173, BR-RELEASE-007) : arbre propre exigé, enregistrement de construction du
//!   client et de l'agent relu par `release-stage`, signature de l'agent par la clé de l'environnement,
//!   racine du dépôt égale à l'espace de travail de GitHub.
//! - `agent-manifest` : section `agent` du `latest.json`, signature vérifiée contre la clé
//!   embarquée dans l'agent (HRT-17, ADR-0021, runbook `mettre-a-jour-agent`).
//!
//! Aucune commande n'est lancée par un interpréteur avec une chaîne construite : les programmes
//! reçoivent des listes d'arguments ; les rares scripts `sh -c` sont des constantes, exécutées
//! dans le conteneur, qui reçoivent leurs valeurs par variables d'environnement.

mod agent;
mod agent_release;
mod agent_sign;
mod br_check;
mod catalog_check;
mod ci;
mod client_record;
mod client_release;
mod docker;
mod e2e;
mod e2e_double_failure;
mod e2e_update;
mod host;
mod release_core;
mod release_github;
mod release_preflight;
mod release_publish;
mod release_stage;
mod repo_root;
mod secret;
mod shellcheck;
mod test_tmp;

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    // Le binaire transmis d'un job à l'autre ne sait où est le dépôt que par son chemin de compilation :
    // en environnement GitHub, ces tâches vérifient d'abord qu'il est celui de l'espace de travail.
    if matches!(
        task.as_deref(),
        Some(
            "client-sign"
                | "client-manifest"
                | "agent-sign"
                | "release-stage"
                | "release-upload"
                | "release-github"
        )
    ) && let Err(message) = repo_root::guard()
    {
        eprintln!("erreur : {message}");
        return ExitCode::FAILURE;
    }
    let result = match task.as_deref() {
        None | Some("help") => {
            println!(
                "tâches :\n  agent         construit le binaire statique de l'agent en conteneur (target/dist/hearth-agent) ; --release : construction de publication (arbre propre exigé avant et après, image téléchargée avec reprise, enregistrement de construction obligatoire)\n  e2e-install   installation de bout en bout dans un conteneur jetable\n  e2e-update    mise à jour de l'agent à distance de bout en bout (retour automatique compris)
  e2e-double-failure  la double panne de la mise à jour de l'agent (superviseur tué, retour arrière sans geste) sur un vrai systemd
  shellcheck    contrôle les scripts de deploy/ avec shellcheck (en conteneur)\n  test-tmp-check  échoue si la suite de tests a laissé des dossiers temporaires (--clean les supprime)\n  br-check      toute référence BR-… du code et des docs a sa fiche
  catalog-check contrôle les fiches de serveur de jeu (chargeur réel, ids.lock) ; --release refuse une fiche preview
  client-release-check  refuse la clé de développement, une version qui n'est pas celle du dépôt et une étiquette vX.Y.Z déjà présente sur origin
  client-version        la version du dépôt (seule source du numéro publié)\n  client-sign           signe l'installateur avec la clé des variables d'environnement (version dans la signature)\n  client-manifest       vérifie la signature puis écrit latest.json (manifeste du greffon de mise à jour du client ; --url = https://hearth.voikyrioh.fr/api/releases/X.Y.Z/Hearth_X.Y.Z_x64-setup.exe) ; sans --notes-file, notes de HEARTH_NOTES écrites dans notes.txt à côté de --out ; sans --date, la date UTC du moment
  release-preflight     contrôles avant toute construction (HRT-172) : branche main, numéros concordants dans les cinq fichiers, notes, clé de développement, HEAD, version non publiée ni étiquette orpheline (env HEARTH_REF, HEARTH_SHA, HEARTH_NOTES, GH_TOKEN ; sorties version et commit) ; --versions-only : numéros et clés seulement, sans réseau (CI de chaque PR)
  release-stage         assemble une version sous target/release-stage/, refuse un arbre qui n'est pas son commit (suivi modifié, non suivi ou ignoré sous les entrées de la construction, fichier masqué) et un agent sans enregistrement de construction (cargo xtask agent) de ce commit, puis enregistre le commit (--client-dir D --agent-binary F --agent-signature S [--install-script P]) après avoir revérifié signatures, sommes, adresses et liste fermée
  release-github        étiquette vX.Y.Z et release GitHub, ce qui manque seulement (--stage-dir D --notes-file F ; env HEARTH_SHA, GH_TOKEN ; commit de COMMIT égal à HEARTH_SHA ; étiquette sur un autre commit = arrêt ; création relue, manifeste public relu)
  release-upload        dépose la version assemblée au bucket (refuse un arbre qui n'est pas son commit ou un autre commit que celui assemblé ; seul git, en lecture, tourne dans l'essai à blanc) : ESSAI À BLANC par défaut, --confirm pour écrire (fichiers, relecture publique, script, manifeste en dernier, chacun relu dans le bucket ; refuse un manifeste en place de version supérieure sauf --allow-rollback ; nécessite l'outil aws et ses variables AWS_* dans l'environnement)
  client-record         enregistrement de construction de l'installateur (HRT-173) : --begin avant la construction (arbre propre exigé), puis --installer F --version X.Y.Z après (écrit F.build : somme, commit, arbre) ; relu par release-stage
  agent-sign            signe le binaire de l'agent (--binary F --version X.Y.Z --out S) avec la clé de HEARTH_AGENT_SIGNING_KEY et son mot de passe HEARTH_AGENT_SIGNING_KEY_PASSWORD (obligatoire) ; commentaire signé version:X.Y.Z ; relue contre update-key.pub
  ci-root-check         en environnement GitHub, la racine du dépôt compilée dans xtask doit être $GITHUB_WORKSPACE
  agent-manifest        vérifie la signature de l'agent contre sa clé embarquée puis ajoute la section `agent` à latest.json (cible de la mise à jour de l'agent)"
            );
            return ExitCode::SUCCESS;
        }
        Some("agent") => agent::run(&std::env::args().skip(2).collect::<Vec<_>>()).map(|_| ()),
        Some("client-record") => client_record::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("agent-sign") => agent_sign::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("ci-root-check") => repo_root::run_check(),
        Some("e2e-install") => e2e::run(),
        Some("e2e-update") => e2e_update::run(),
        Some("e2e-double-failure") => e2e_double_failure::run(),
        Some("shellcheck") => shellcheck::run(),
        Some("test-tmp-check") => test_tmp::run(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("br-check") => return br_check::run(),
        Some("catalog-check") => {
            return catalog_check::run(&std::env::args().skip(2).collect::<Vec<_>>());
        }
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
        Some("release-preflight") => {
            match release_preflight::run(&std::env::args().skip(2).collect::<Vec<_>>()) {
                Ok(()) => Ok(()),
                // Déjà annoncé en annotation de l'exécution.
                Err(release_preflight::RunError::Announced) => return ExitCode::FAILURE,
                Err(release_preflight::RunError::Failed(message)) => Err(message),
            }
        }
        Some("release-stage") => {
            release_stage::run_stage(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("release-upload") => {
            release_publish::run_upload(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some("release-github") => {
            release_github::run(&std::env::args().skip(2).collect::<Vec<_>>())
        }
        Some(other) => Err(format!("tâche inconnue : {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            // Mode GitHub, tâches de publication : la PREMIÈRE ligne (le message de la spec) en annotation
            // et au résumé, le détail technique sur les lignes suivantes (journal seulement).
            if matches!(task.as_deref(), Some("release-upload" | "release-github")) {
                let (first, detail) = message.split_once('\n').unwrap_or((message.as_str(), ""));
                if ci::fail(first) {
                    if !detail.is_empty() {
                        eprintln!("{detail}");
                    }
                    return ExitCode::FAILURE;
                }
            }
            eprintln!("erreur : {message}");
            ExitCode::FAILURE
        }
    }
}
