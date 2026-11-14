---
id: BR-MONDE-011
domaine: MONDE
titre: La copie se fait en flux, sur un fil dédié à priorité basse, avec progression, et dure six heures au plus
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-011 — La copie se fait en flux, sur un fil dédié à priorité basse, avec progression, et dure six heures au plus

## Règle
La copie lit par morceaux de 256 Kio (jamais un fichier entier en mémoire), tourne sur un fil créé pour elle dont le « nice » est abaissé, publie fichiers et octets copiés sur le total une fois par seconde, et s'arrête à six heures (`Cause::Timeout`), retirant ce qu'elle a copié.

## Application (code)
- `crates/hearth-agent/src/application/world_import/copy.rs::copy_world`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::import_world`
- `crates/hearth-agent/src/domain/worlds/mod.rs::COPY_MAX`

## Vérification
- `crates/hearth-agent/src/application/world_import/copy.rs::tests::a_cancel_or_a_past_deadline_stops_the_copy`

## Cas limites
- Le « nice » n'est pas une priorité d'entrée-sortie (`ioprio_set` demande `libc` et `unsafe`) ; même limite que l'effacement (`BR-DEL-007`).

## Règles liées
- BR-MONDE-013

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
