---
id: BR-FICHE-010
domaine: FICHE
titre: Ce que l'image télécharge au premier démarrage est déclaré avec sa source et la preuve de sa taille
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-010), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-010 — Ce que l'image télécharge au premier démarrage est déclaré avec sa source et la preuve de sa taille

## Règle
`[first_start]` déclare le délai « prêt » (de 60 secondes à 6 heures), ce que l'image télécharge (`what`, `source_label`, `bytes` et `evidence`) et des repères d'étape littéraux. Un `bytes` accompagné de `unmeasured` est refusé. Hearth ne vérifie pas ce que l'image télécharge.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_first_start`
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::FirstStart`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i5_no_address_is_contacted_https_only (adresse de la preuve)`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i6_neither_regular_expression_nor_template_signs_and_versions_are_literals (repères d'étape)`

## Cas limites
- L'affichage (octets écrits, jamais un pourcentage) est côté client et phases (HRT-101, HRT-102).

## Règles liées
- BR-FICHE-008

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
