---
id: BR-FICHE-022
domaine: FICHE
titre: L'étape « Ressources » dit en chiffres ce qui reste de mémoire quand la machine est assez grande mais que la recommandation ne tient pas
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-022), HRT-101
maj: 2026-10-24
---

# BR-FICHE-022

## Règle
Règle de présentation, portée par le client (HRT-102) : la phrase « Il reste {x} de mémoire disponible sur ton serveur ({y} recommandés pour ce jeu). » se calcule avec `GET /engine/capacity` et la mémoire recommandée du détail de la fiche, dont la preuve est rendue par `GET /catalog/{id}`. Côté agent, les avertissements sans blocage d'un minimum annoncé par l'éditeur ou non mesuré sont `fit_warnings`.

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs::fit_warnings`
- `crates/hearth-agent/src/application/catalog.rs::CatalogService::detail`

## Vérification
- `crates/hearth-agent/src/domain/services/from_fiche.rs::an_unmeasured_minimum_never_blocks`

## Cas limites
- Le texte et la jauge sont au client (HRT-102).

## Règles liées
- BR-FICHE-021, BR-RES-007

## Historique
- 2026-10-24 — création (HRT-101).
