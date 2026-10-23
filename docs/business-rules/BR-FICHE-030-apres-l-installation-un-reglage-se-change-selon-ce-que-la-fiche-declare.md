---
id: BR-FICHE-030
domaine: FICHE
titre: Après l'installation, un réglage n'est modifiable que si la fiche le déclare ; un réglage figé à la création du monde ne se change pas
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-030), HRT-101
maj: 2026-10-24
---

# BR-FICHE-030

## Règle
La description garde, pour chaque réponse, si la question est figée à la création du monde (`frozen`). La modification d'un réglage (onglet « Réglages », retouche d'une ligne du fichier sans le réécrire) est portée par HRT-108 et HRT-109 : cette fiche est créée ici avec le pointeur sur la donnée qu'elles liront, et son état « non couvert ici » est assumé.

## Application (code)
- `crates/hearth-agent/src/domain/services/spec.rs::SpecAnswer`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::body_and_secrets_from`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::the_fiche_description_keeps_answers_and_frozen_questions`

## Cas limites
- Non couvert ici : la modification d'un réglage (HRT-108, HRT-109).

## Règles liées
- BR-FICHE-029, BR-EDIT-035

## Historique
- 2026-10-24 — création (HRT-101).
