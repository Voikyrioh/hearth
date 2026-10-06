---
id: BR-UPDATE-014
domaine: UPDATE
titre: L'étape « redémarrage » est annoncée avant l'arrêt de l'agent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-014), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-014 : L'étape « redémarrage » est annoncée avant l'arrêt de l'agent

## Règle
Côté agent : avant de lancer le superviseur, l'agent diffuse l'étape `restart` ; le superviseur laisse 2 secondes à l'ancien agent pour la livrer avant de l'arrêter. Pendant le redémarrage, le lien tombe : l'état « Reconnexion… » et l'absence d'alarme sont l'affaire du client (`hearth-link`, BR-RESIL). Quand le nouvel agent revient, l'étape en cours (`check`) se relit par `GET /agent/update` et par l'abonnement au sujet `update`.

## Application (code)
- `crates/hearth-agent/src/application/update.rs::UpdateService::{execute, progress, resume}`.
- `crates/hearth-agent/src/domain/update/supervise.rs::STOP_GRACE`, `application/update_supervisor.rs::Supervisor::run` (écrit l'étape `restart` puis `check`).

## Vérification
- `tests/update_use_cases.rs` (étape `restart`), `a_supervisor_that_still_works_counts_as_an_update_in_progress_after_a_restart`.

## Interface (HRT-17, lot interface)
- Quand l'agent annonce `restart`, la bibliothèque de liaison (`hearth-link`) rend la coupure qui suit ATTENDUE pendant `RESTART_WINDOW` (2 minutes) : « Reconnexion… » dès la coupure, jamais « Hors ligne », aucun échec compté (donc ni notification Windows de panne, BR-RESIL-015, ni avis « reconnexion échouée », BR-RESIL-018). Le retour du lien ou l'étape `done` lève l'attente ; passé la fenêtre sans retour, c'est une panne ordinaire. L'agent ferme le flux par le code 1001 (« l'agent s'arrête »), déjà distingué d'une fin délibérée de session. L'écran garde ses étapes pendant la coupure et le dit (« Le lien avec le serveur sera coupé brièvement pendant le redémarrage. »).
- Code : `crates/hearth-link/src/domain/state.rs` (`Input::{RestartAnnounced, RestartEnded}`, `Thresholds::restart_window`, `LinkMachine::offline_at`), `crates/hearth-link/src/manager/task.rs::on_update`.
- Tests : `crates/hearth-link/src/domain/state/tests.rs` (`an_announced_restart_shows_reconnecting_at_once_and_never_offline_inside_the_window`, `the_new_agent_answering_ends_the_expected_cut_and_the_next_cut_is_an_ordinary_one`, `the_end_of_the_update_lifts_the_expectation_and_a_stopped_link_forgets_it`, `the_expected_cut_has_exact_deadlines_and_none_is_left_over`) ; `crates/hearth-link/tests/agent_update.rs::a_restart_announced_by_the_agent_is_an_expected_cut_then_the_result_is_read_back` (vrai agent) et son témoin `an_ordinary_cut_with_the_same_thresholds_is_offline_at_once` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts`.

## Cas limites
- L'affichage « Reconnexion… » n'est pas du ressort de l'agent.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
