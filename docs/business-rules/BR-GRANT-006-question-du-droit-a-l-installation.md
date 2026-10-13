---
id: BR-GRANT-006
domaine: GRANT
titre: À l'installation, quand un moteur de conteneurs est détecté, la question du droit est posée pour le premier compte ; sans moteur elle ne l'est pas
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-006), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-77
maj: 2026-10-13
---

# BR-GRANT-006 — À l'installation, quand un moteur de conteneurs est détecté, la question du droit est posée pour le premier compte ; sans moteur elle ne l'est pas

## Règle
Après la détection du moteur et avant la création du premier compte, `hearth-agent install` affiche l'aide « Un compte qui peut gérer les services choisit des images et les fait tourner sur ton serveur. » puis pose « Ce compte pourra-t-il déployer et piloter des services sur ce serveur ? [O/n] » à propos du premier compte. Entrée, `o`, `O` : oui ; `n`, `N` : non ; toute autre réponse affiche « Réponds O ou N. » et repose la question UNE fois ; une deuxième réponse invalide vaut non. Une entrée qui se ferme (fin de fichier) vaut non : jamais un octroi par défaut. Le résultat est dit après la réussite : « Le compte {compte} peut maintenant gérer les services de ce serveur. » ou « Le compte {compte} ne peut pas gérer les services de ce serveur. Tu pourras l'accorder plus tard avec : hearth-agent account services {compte} --grant ».

Moteur détecté = pilotage permis (`engine.enabled`) et repère d'installation (`dockerd`, unité `docker.service` ou `docker.socket`) ou socket du moteur présent : métadonnées seulement, rien n'est exécuté ni ouvert. Sans moteur détecté : la question n'est pas posée, le droit n'est pas accordé, même avec `--services`, et l'installation dit « Aucun moteur de conteneurs détecté : le droit de gérer les services n'est pas accordé. Tu pourras l'accorder plus tard avec : hearth-agent account services {compte} --grant ».

La question ne concerne que le PREMIER compte, créé par cette installation : une réinstallation ou une mise à jour avec des comptes existants ne pose rien et n'accorde rien (BR-GRANT-002).

## Application (code)
- `crates/hearth-agent/src/entrypoint/install.rs::{decide_services, ask_services}`
- `crates/hearth-agent/src/infrastructure/engine/installed.rs::engine_detected`
- `crates/hearth-agent/src/app/install.rs::LocalAdmins::create_admin` (octroi consigné, BR-GRANT-009)

## Vérification
- `crates/hearth-agent/tests/install_flow.rs::enter_grants_the_right_to_the_first_account_when_an_engine_is_detected`
- `crates/hearth-agent/tests/install_flow.rs::o_grants_and_n_does_not_and_says_how_to_grant_later`
- `crates/hearth-agent/tests/install_flow.rs::an_invalid_answer_asks_again_and_a_second_invalid_answer_means_no`
- `crates/hearth-agent/tests/install_flow.rs::an_input_that_closes_means_no_and_never_a_grant`
- `crates/hearth-agent/tests/install_flow.rs::without_an_engine_nothing_is_asked_nothing_is_granted_and_the_command_is_given`
- `crates/hearth-agent/tests/install_flow.rs::an_update_never_asks_nor_grants_to_the_accounts_that_already_exist`
- `crates/hearth-agent/src/infrastructure/engine/installed.rs::tests::an_engine_is_detected_by_its_markers_or_its_socket_unless_the_piloting_is_off`
- `deploy/e2e/scenario.sh` (section « Droit de gérer les services », CI seulement : `cargo xtask e2e-install`)

## Cas limites
- Pilotage désactivé dans `agent.toml` : traité comme « aucun moteur » (le droit n'ouvrirait rien).
- Le moteur est installé après l'installation de l'agent : le droit n'est pas accordé ; la commande de BR-GRANT-008 l'accorde.
- Si l'octroi échoue après la création du compte, le compte n'est pas laissé derrière et l'installation échoue (retour en arrière).

## Règles liées
- BR-GRANT-002, BR-GRANT-007, BR-GRANT-008, BR-GRANT-009

## Historique
- 2026-10-13 — création (HRT-77, ADR-0041).
