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
- `apps/desktop/src/fiche/messages.ts::downloadText`
- `apps/desktop/src/fiche/firstStart.ts`
- `apps/desktop/src/components/organisms/FirstStartPanel.vue`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i5_no_address_is_contacted_https_only (adresse de la preuve)`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i6_neither_regular_expression_nor_template_signs_and_versions_are_literals (repères d'étape)`
- `apps/desktop/src/fiche/fiche.test.ts`
- `apps/desktop/src/pages/FicheDetail.test.ts`

## Cas limites
- Le détail de la fiche rend les téléchargements (source, taille, preuve) et les repères d'étape ; la phrase de la fiche est dite par le client (HRT-102). Le relevé des octets écrits dans le détail du service dépend de la projection de la phase par l'agent (suivi HRT-197).

## Règles liées
- BR-FICHE-008

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-24 — côté client (HRT-102).
