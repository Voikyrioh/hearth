---
id: BR-ADOPT-024
domaine: ADOPT
titre: Deux comptes peuvent adopter ou relâcher en même temps : chaque service n'est adopté ou relâché qu'une fois, par le premier acte arrivé au serveur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-024), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-024 — Deux comptes peuvent adopter ou relâcher en même temps : chaque service n'est adopté ou relâché qu'une fois, par le premier acte arrivé au serveur

## Règle
Les actes d'adoption et de relâche sont sérialisés dans l'agent (un verrou par service d'adoption) ET la base refuse la double écriture (`INSERT … ON CONFLICT DO NOTHING` dans une transaction `BEGIN IMMEDIATE`) : deux actes simultanés sur la même clé produisent une adoption (`adopted`) et un `already_adopted` (par le premier compte), une seule entrée « Service adopté ». Même règle pour relâcher : le second voit `released: false`.

## Application (code)
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption`
- `crates/hearth-agent/src/infrastructure/sqlite/adoption_repo.rs::adopt`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::two_accounts_adopting_the_same_service_at_the_same_time_adopt_it_once`

## Cas limites
- Le verrou et la contrainte de la base se couvrent : la suppression de l'un des deux laisse l'essai vert, la double protection est voulue.

## Règles liées
- BR-ADOPT-010, BR-ADOPT-013

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
