---
id: BR-MOD-027
domaine: MOD
titre: Un serveur qui ne démarre pas avec ses nouveaux mods revient seul à l'état d'avant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-027 : Retour automatique

## Règle
Après la mise en place, le service est relancé puis surveillé : succès = le signe « prêt » de la fiche (`ready.log_contains`, sous-chaîne littérale) dans les lignes écrites depuis la relance, avant `first_start.ready_timeout_s` ; échec = conteneur sorti avant le signe, ou délai dépassé. Sans signe essayé sur la vraie image, le conteneur doit rester en marche quelques secondes. En cas d'échec Hearth NE relance PAS en boucle : il arrête le service, remet les mods d'avant (les mouvements du journal rejoués à l'envers, l'inode vérifié ; un fichier posé par le lot part dans « Mods retirés », rien n'est supprimé), relance, et l'opération finit `rolled_back` (cause : `stopped_right_after_start` ou `timeout`). Une erreur au milieu de `place` défait de la même façon les mouvements déjà faits et finit `failed` (`step_failed`). **Le retour est surveillé comme la première relance** (même signe, même délai) : si le jeu ne revient pas non plus avec les mods d'avant, il est arrêté (arrêt voulu inscrit) et rien n'est relancé seul : `interrupted` (`step_failed`). L'arrêt du retour est un arrêt VOULU, inscrit en base et dans le registre comme tout arrêt demandé par Hearth : le service ne passe jamais par « En erreur » et sa règle de redémarrage ne le relance pas. Avant d'arrêter, l'agent lit les 20 dernières lignes du jeu et les garde 24 heures EN MÉMOIRE seulement (`GET /operations/services/{id}/tail`, jamais au disque ni au journal d'activité) avec la cause qu'il reconnaît par des sous-chaînes littérales d'une liste fermée (`duplicate_mod`, `missing_dependency`, `incompatible`, `out_of_memory`) et sa phrase en clair (« Un même mod est installé deux fois... », « Un des mods a besoin d'un autre mod qui est absent... »). Aucune ligne du jeu n'est dans une trace, une erreur ou le journal d'activité. Une coupure de l'agent pendant l'opération est classée sans rien rejouer seul (`recover`, BR-OPS) ; « Revenir à l'état d'avant » et « J'ai vérifié » sont les choix d'une opération « à vérifier ».

## Application (code)
- Client (HRT-263) : `apps/desktop/src/composables/useModsApply.ts` (dernières lignes du journal), `ModsProgress.vue` (échec revenu à l'état d'avant, sans choix)
- `crates/hearth-agent/src/application/mods_apply.rs::{ModsService::watch, ModsService::roll_back, revert_blocking}`
- `crates/hearth-agent/src/domain/ops/plan.rs::{FileMove, MoveKind}` (le journal des mouvements, inode compris)
- `crates/hearth-agent/src/domain/mods/failure.rs::{FailureReason, reason_of}` (cause reconnue, phrase en clair)
- `crates/hearth-agent/src/application/mods_apply.rs::{keep_tail_of, stop_for_rollback}` et `Backups::stop_again_for_mods` (arrêt voulu inscrit)
- `crates/hearth-agent/src/application/mods.rs::ModsService::tail` (24 heures en mémoire)

## Vérification
- `tests/mods_apply_https.rs` (conteneur sorti avant le signe, délai dépassé, erreur au milieu de `place`, `the_last_lines_of_the_game_and_the_plain_cause_are_kept_after_a_rollback`, `an_unrecognised_failure_keeps_the_lines_without_a_cause`, `a_return_that_does_not_come_up_either_is_interrupted_stops_the_game_and_relaunches_nothing`)
- `crates/hearth-agent/src/domain/mods/failure.rs::tests`

## Règles liées
- BR-MOD-024, BR-MOD-025, BR-MOD-032, BR-EDIT-020

## Historique
- 2026-11-18 : complétée (HRT-259) : surveillance après le retour, arrêt voulu inscrit pendant le retour, queue de 20 lignes gardée 24 h et phrase en clair.
- 2026-11-18 : création (HRT-258 pour l'erreur au milieu de `place`, HRT-259 pour le retour automatique).
