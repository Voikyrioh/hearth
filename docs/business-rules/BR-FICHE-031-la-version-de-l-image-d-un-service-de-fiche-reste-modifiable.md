---
id: BR-FICHE-031
domaine: FICHE
titre: La version de l'image d'un service créé par fiche reste modifiable par « modifier un service » ; une version hors fiche est avertie, pas bloquée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-031), HRT-101
maj: 2026-10-24
---

# BR-FICHE-031

## Règle
Règle de « modifier un service » (HRT-121 et suivants), créée ici pour que le lien fiche-service la porte : la description garde la fiche, sa version et son empreinte (BR-FICHE-029), de quoi comparer à la fiche livrée avec l'agent actuel. Non couvert ici.

## Application (code)
- `crates/hearth-agent/src/domain/services/spec.rs::SpecFiche`

## Vérification
- `crates/hearth-agent/src/domain/services/spec.rs::a_fiche_description_keeps_fiche_version_digest_answers_agreements_and_frozen_questions`

## Cas limites
- Non couvert ici : la modification d'un service de fiche (HRT-121).

## Règles liées
- BR-FICHE-029

## Historique
- 2026-10-24 — création (HRT-101).
