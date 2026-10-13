---
id: BR-DEL-019
domaine: DEL
titre: Un nom en cours de suppression est réservé : un déploiement du même nom est refusé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEL-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-DEL-019 — Un nom en cours de suppression est réservé : un déploiement du même nom est refusé

## Règle
Pendant une suppression (et un déploiement), le nom du service (replié) est réservé dans la table des verrous ; une seconde demande du même nom est refusée avec le code `NAME_BEING_DELETED` quand c'est une suppression qui le tient. Après un redémarrage, le nom d'une suppression « à vérifier » reste réservé (rebâti au démarrage, BR-LIFE-013). Le nom de la description courante est de plus unique en base (index unique partiel sur le nom replié).

## Application (code)
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable::reserve_name`
- `crates/hearth-agent/src/domain/ops/locks.rs::Refusal::error_code`
- `crates/hearth-agent/src/application/service_ops.rs::AcceptRequest`

## Vérification
- `crates/hearth-agent/tests/service_ops_journal.rs::a_name_being_deleted_is_reserved_and_a_file_write_is_refused_under_an_exclusive_lock`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_name_and_a_heavy_seat_survive_a_restart_and_reverting_only_starts_the_rollback`
- `crates/hearth-agent/src/domain/ops/locks.rs::tests::a_name_being_deleted_is_reserved_and_says_so`

## Cas limites
- Le texte « Le service {nom} est en cours de suppression… » vient avec la suppression (HRT-85).

## Règles liées
- BR-LIFE-013, BR-EDIT-035

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
