---
id: BR-MONDE-009
domaine: MONDE
titre: Un fichier dont la taille ou la date change pendant la copie est noté ; la copie est retirée, ou gardée si le compte l'a demandé d'avance
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-15
---

# BR-MONDE-009 — Un fichier dont la taille ou la date change pendant la copie est noté ; la copie est retirée, ou gardée si le compte l'a demandé d'avance

## Règle
Chaque fichier est relevé avant et après sa lecture ; un changement, ou un fichier disparu, est compté. Sans « garder cette copie » d'avance (`keep_if_changed`), la copie est retirée et rien n'est créé ; avec, le service est créé et le journal dit « copie gardée ».

## Application (code)
- `crates/hearth-agent/src/application/world_import/copy.rs::copy_world`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::import_world`
- `apps/desktop/src/composables/useFicheInstall.ts::setKeepIfChanged` (« Garder la copie si des fichiers changent », décoché par défaut, part avec la demande)

## Vérification
- `crates/hearth-agent/src/application/world_import/copy.rs::tests::a_file_that_vanishes_is_noted_changed_not_failed`
- `crates/hearth-agent/tests/world_import_https.rs::a_file_changed_during_the_copy_is_discarded_unless_the_copy_was_kept_in_advance`
- `apps/desktop/src/pages/InstallWorld.test.ts` (installer avec un monde)

## Cas limites
- Le repère « Peut être incohérente » sur le service créé n'est pas gardé : seul le journal d'activité le dit (suivi client, HRT-102).

## Règles liées
- BR-MONDE-010

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
- 2026-11-15 — côté client (HRT-106).
