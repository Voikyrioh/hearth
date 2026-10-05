---
id: BR-UPDATE-028
domaine: UPDATE
titre: Un travail de mise à jour laissé en cours est conclu au démarrage
statut: active
invariant: true
source: revue Stephen de HRT-17 (round 1), ADR-0014
maj: 2026-10-05
---

# BR-UPDATE-028 : Un travail de mise à jour laissé en cours est conclu au démarrage

## Règle
Dès sa demande, la mise à jour laisse une trace (`update/state.json` : version, étape, qui a demandé), puis `job.json` au lancement du superviseur, puis la sauvegarde de l'ancien binaire à l'échange. Au démarrage, sans superviseur vivant ni résultat à annoncer, l'agent croise ces traces avec **la version qui tourne** et conclut selon l'étape atteinte (une trace illisible n'est jamais lue comme absente : `failed` / `interrupted`, ou `rollback_failed` si l'ancien binaire est gardé, copies conservées pour la reprise à la main) ; une demande en cours dans ce processus n'est jamais prise pour un orphelin : **avant le lancement du superviseur, ou superviseur qui n'a jamais échangé** : abandon propre (dépôt nettoyé), résultat `failed` / `interrupted`, entrée au journal au nom de qui avait demandé ; **la nouvelle version tourne déjà** (réussite dont le résultat n'a pas été écrit) : `succeeded` ; **la version d'avant tourne déjà alors que la sauvegarde est restée** (retour arrière ou reprise à la main) : `rolled_back` et traces retirées, **sans jamais toucher à la base** ; **après l'échange** : un superviseur de reprise (`Job::recover`) contrôle le binaire en place (nouvelle version, même certificat) et, sinon, remet exactement l'ancien binaire et la base d'avant (BR-UPDATE-029). Le résultat est écrit, annoncé sur le flux, consigné au journal, et le verrou est libéré. Le verrou est lu **avant** le résultat : un superviseur qui conclut entre les deux lectures est rattrapé par la surveillance.

## Application (code)
- `crates/hearth-agent/src/domain/update/orphan.rs::classify_orphan` (fonction pure : traces, superviseur vivant ou non, version courante).
- `crates/hearth-agent/src/application/update.rs::UpdateService::{write_intent, resume, abandon, recover}`.
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{recover, check}`.

## Vérification
- `domain::update::orphan::tests` (une étape par cas).
- `tests/update_use_cases.rs` : `an_agent_killed_while_downloading_is_concluded_as_interrupted_at_the_next_start`, `a_supervisor_that_never_swapped_is_concluded_as_interrupted_too`, `a_swap_nobody_concluded_is_taken_over_by_a_recovery_supervisor`, `the_result_of_a_recovery_is_announced_and_journaled_once_it_is_written`, `a_supervisor_that_concludes_while_the_agent_starts_is_still_announced`, `the_intent_is_written_as_soon_as_the_update_is_asked_and_follows_every_step`.
- `tests/update_supervisor.rs` : `a_recovery_keeps_a_new_agent_that_answers_and_never_stops_the_service`, `a_recovery_puts_back_the_exact_old_binary_and_database_when_the_new_agent_does_not_hold`.
- `deploy/e2e/scenario-update.sh` (superviseur tué après l'échange, agent redémarré).

## Cas limites
- Si le nouveau binaire ne démarre pas du tout (superviseur tué, puis serveur redémarré), personne ne tourne pour conclure : reprise à la main (runbook).
- Un résultat `rollback_failed` laisse les traces : le démarrage suivant retente la reprise.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-018, ADR-0014

## Historique
- 2026-10-05 : création (HRT-17, suite de la revue de code).
