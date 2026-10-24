---
id: BR-FICHE-025
domaine: FICHE
titre: Le récapitulatif « Vérifie avant de lancer » montre fiche, image, ports, dossiers, réponses (secrets masqués) et ressources
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-025), HRT-101
maj: 2026-10-24
---

# BR-FICHE-025

## Règle
Règle de présentation, portée par le client (HRT-102) à partir du détail de la fiche (`GET /catalog/{id}`) : l'image exacte avec son empreinte, les ports, les dossiers avec leur chemin dans le service, les ressources avec leur preuve, les contrats. Les secrets sont masqués ; ils ne sont jamais renvoyés par l'agent (le détail n'a pas de valeur secrète).

## Application (code)
- `crates/hearth-agent/src/application/catalog.rs::CatalogService::detail`

## Vérification
- `crates/hearth-agent/src/application/catalog.rs::the_detail_shows_what_the_fiche_imposes_with_the_proof_of_each_figure_and_no_secret`

## Cas limites
- Le récapitulatif lui-même est au client (HRT-102).

## Règles liées
- BR-FICHE-004, BR-FICHE-026

## Historique
- 2026-10-24 — création (HRT-101).
