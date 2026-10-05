---
id: BR-UPDATE-014
domaine: UPDATE
titre: L'étape « redémarrage » est annoncée avant l'arrêt de l'agent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-014), HRT-17
maj: 2026-10-05
---

# BR-UPDATE-014 : L'étape « redémarrage » est annoncée avant l'arrêt de l'agent

## Règle
Côté agent : avant de lancer le superviseur, l'agent diffuse l'étape `restart` ; le superviseur laisse 2 secondes à l'ancien agent pour la livrer avant de l'arrêter. Pendant le redémarrage, le lien tombe : l'état « Reconnexion… » et l'absence d'alarme sont l'affaire du client (`hearth-link`, BR-RESIL). Quand le nouvel agent revient, l'étape en cours (`check`) se relit par `GET /agent/update` et par l'abonnement au sujet `update`.

## Application (code)
- `crates/hearth-agent/src/application/update.rs::UpdateService::{execute, progress, resume}`.
- `crates/hearth-agent/src/domain/update/supervise.rs::STOP_GRACE`, `application/update_supervisor.rs::Supervisor::run` (écrit l'étape `restart` puis `check`).

## Vérification
- `tests/update_use_cases.rs` (étape `restart`), `a_supervisor_that_still_works_counts_as_an_update_in_progress_after_a_restart`.

## Cas limites
- L'affichage « Reconnexion… » n'est pas du ressort de l'agent.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
