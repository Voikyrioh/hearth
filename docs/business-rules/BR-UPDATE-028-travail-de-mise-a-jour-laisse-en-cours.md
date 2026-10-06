---
id: BR-UPDATE-028
domaine: UPDATE
titre: Un travail de mise à jour laissé en cours est conclu au démarrage
statut: active
invariant: true
source: revue Stephen de HRT-17 (round 1), ADR-0014
maj: 2026-10-06
---

# BR-UPDATE-028 : Un travail de mise à jour laissé en cours est conclu au démarrage

## Règle
Dès sa demande, la mise à jour laisse une trace (`update/state.json` : version, étape, qui a demandé), puis `job.json` au lancement du superviseur, puis la sauvegarde de l'ancien binaire à l'échange. Au démarrage, sans superviseur vivant ni résultat à annoncer, l'agent croise ces traces avec **la version qui tourne** et conclut selon l'étape atteinte (une trace illisible n'est jamais lue comme absente : `failed` / `interrupted`, ou `rollback_failed` si l'ancien binaire est gardé, copies conservées pour la reprise à la main) ; une demande en cours dans ce processus n'est jamais prise pour un orphelin : **avant le lancement du superviseur, ou superviseur qui n'a jamais échangé** : abandon propre (dépôt nettoyé), résultat `failed` / `interrupted`, entrée au journal au nom de qui avait demandé ; **la nouvelle version tourne déjà** (réussite dont le résultat n'a pas été écrit) : `succeeded` ; **la version d'avant tourne déjà alors que la sauvegarde est restée** (retour arrière ou reprise à la main) : `rolled_back` et traces retirées, **sans jamais toucher à la base** ; **la sauvegarde est restée et la version visée tourne** (échange non conclu) : un superviseur de reprise (`Job::recover`) contrôle le binaire en place (nouvelle version, même certificat) et, sinon, remet exactement l'ancien binaire et la base d'avant (BR-UPDATE-029) ; **la sauvegarde est restée et une TROISIÈME version tourne** (ni la visée ni celle d'avant : quelqu'un a remplacé le binaire à la main) : ni reprise ni retour arrière, **sans jamais toucher à la base ni au binaire**, mise à jour conclue `failed` / `interrupted`, sauvegarde, copie de la base et traces retirées (FIX-01M47N6Z485TWN2H770KQ5H80R). **La patience de la surveillance suit la même règle** : un superviseur absent depuis 30 secondes (ou disparu sans résultat) n'est jamais conclu « échec du superviseur » à l'aveugle ; la surveillance relit les traces et applique `classify_orphan` (échange fait : reprise, version visée en place : réussite, etc.) ; seul « rien n'a été échangé » vaut `failed` / `supervisor_launch` (FIX-01M47PHYR8MD87HAXY9PARQXAN). Le résultat est écrit, annoncé sur le flux, consigné au journal, et le verrou est libéré. Le verrou est lu **avant** le résultat : un superviseur qui conclut entre les deux lectures est rattrapé par la surveillance.

## Application (code)
- `crates/hearth-agent/src/domain/update/orphan.rs::classify_orphan` (fonction pure : traces, superviseur vivant ou non, version courante).
- `crates/hearth-agent/src/application/update.rs::UpdateService::{write_intent, resume, give_up_on_supervisor, conclude_orphan, abandon, recover}`.
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{recover, check}`.

## Vérification
- `domain::update::orphan::tests` (une étape par cas).
- `tests/update_use_cases.rs` : `a_supervisor_that_dies_after_the_swap_is_taken_over_never_declared_failed`, `a_supervisor_that_never_shows_up_ends_as_a_launch_failure_after_the_patience`, `a_third_version_put_in_by_hand_is_never_rolled_back_over_nor_its_database_restored`, `an_agent_killed_while_downloading_is_concluded_as_interrupted_at_the_next_start`, `a_supervisor_that_never_swapped_is_concluded_as_interrupted_too`, `a_swap_nobody_concluded_is_taken_over_by_a_recovery_supervisor`, `the_result_of_a_recovery_is_announced_and_journaled_once_it_is_written`, `a_supervisor_that_concludes_while_the_agent_starts_is_still_announced`, `the_intent_is_written_as_soon_as_the_update_is_asked_and_follows_every_step`.
- `tests/update_supervisor.rs` : `a_recovery_keeps_a_new_agent_that_answers_and_never_stops_the_service`, `a_recovery_puts_back_the_exact_old_binary_and_database_when_the_new_agent_does_not_hold`.
- `deploy/e2e/scenario-update.sh` (superviseur tué après l'échange, agent redémarré).

## Cas limites
- Si le nouveau binaire ne démarre pas du tout (superviseur tué, puis serveur redémarré), personne ne tourne pour conclure : reprise à la main (runbook ; étude d'une unité de reprise dans l'ADR-0014, 2026-10-06).
- **Une version illisible dans le travail** (visée ou d'avant), avec la sauvegarde présente : jamais de reprise ni de retour arrière (une reprise attendrait une version qui ne répondra pas, puis remettrait une copie périmée de la base) : conclu `failed` / `rollback_failed`, version inconnue, **copies gardées**, entrée au journal ; reprise à la main (runbook). Sans sauvegarde et version visée illisible : `failed` / `interrupted`.
- **Une reprise est tentée UNE fois par échange** : le travail de reprise est écrit avec `recover` ; une reprise qui n'a rien conclu (superviseur de reprise muet, tué) n'est jamais relancée, ni par la patience ni par un redémarrage : conclu `failed` / `rollback_failed`, copies (ancien binaire, base d'avant) **gardées** pour la reprise à la main, traces de travail (`job.json`, `state.json`) retirées, entrée au journal, verrou libéré. Sans cela, une reprise muette bouclerait (régression relevée en review de la PR #21).
- Par `install.sh --binary`, l'installation utilise le même chemin de sauvegarde et le retire à la fin : le démarrage suivant voit « pas de sauvegarde, une autre version tourne » (`LaunchedNoSwap`), pas `ForeignVersion` ; même résultat visible (`failed` / `interrupted`, base intacte, copies retirées). `ForeignVersion` ne sert que pour un binaire copié à la main alors que la sauvegarde existe.
- La patience de la surveillance conclut un superviseur qui a travaillé sans rien échanger comme le démarrage (`failed` / `interrupted`, avec les versions du travail) ; `supervisor_launch` ne reste que si le travail n'a jamais été écrit.

## Table de vérité de `classify_orphan`
Voir la tâche T27 (une ligne = un test `row_NN_*` de `domain::update::orphan::tests`, plus `no_combination_loops_or_restores_a_stale_copy`, exhaustif). Seul `AfterSwap` peut remettre la base, par le retour arrière de CE travail, une seule fois.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-018, ADR-0014

## Historique
- 2026-10-05 : création (HRT-17, suite de la revue de code).
- 2026-10-06 : la patience de la surveillance passe par `classify_orphan` ; cas d'une troisième version (FIX-01M47N6Z485TWN2H770KQ5H80R).
