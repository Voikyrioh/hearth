---
id: BR-LIFE-013
domaine: LIFE
titre: Une seule opération à la fois par service ; les écritures de fichiers se partagent, les lectures n'en prennent aucun
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-LIFE-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-LIFE-013 — Une seule opération à la fois par service ; les écritures de fichiers se partagent, les lectures n'en prennent aucun

## Règle
Toute opération (démarrer, arrêter, redémarrer, déployer, modifier, supprimer, ressources, sauvegarder, restaurer, reprendre un monde) prend le verrou EXCLUSIF de son service à l'acceptation et le rend à sa fin. Une écriture de fichier prend un verrou PARTAGÉ avec les autres écritures et est refusée sous un exclusif ; une lecture n'en prend aucun (sauf la lecture de fichiers pendant une restauration, BR-FILES-024). Deux verrous du serveur : `heavy_io` (une place : sauvegarde, restauration, reprise de monde) et les noms réservés (BR-DEL-019). Le verrou est en mémoire, rebâti au démarrage pour les opérations « à vérifier » (BR-EDIT-031) : verrou du service, nom réservé (suppression, déploiement) et place lourde (sauvegarde, restauration, reprise de monde), déduits du genre. Ceinture en base : un index unique partiel interdit deux opérations non terminées sur un même service.

## Application (code)
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable`
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::lock_mode`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::accept`
- `crates/hearth-agent/migrations/0012_service_specs_and_ops.sql (index service_ops_open)`

## Vérification
- `crates/hearth-agent/src/domain/ops/locks.rs::tests`
- `crates/hearth-agent/src/domain/ops/kind.rs::tests::the_statuses_that_hold_the_lock_are_the_ones_the_unique_index_covers`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_accepted_operation_takes_the_service_and_gives_it_back_when_it_ends`
- `crates/hearth-agent/tests/service_ops_journal.rs::the_unique_index_is_the_belt_when_the_memory_lock_has_a_defect`
- `crates/hearth-agent/tests/service_ops_journal.rs::heavy_io_has_one_seat_for_the_whole_server`

## Cas limites
- Le message « Une action est déjà en cours sur ce service. » et le nom du compte viennent avec les routes (HRT-64).

## Règles liées
- BR-FILES-024, BR-DEL-019, BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
