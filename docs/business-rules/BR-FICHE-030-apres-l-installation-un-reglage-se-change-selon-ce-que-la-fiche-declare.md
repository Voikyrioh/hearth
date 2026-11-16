---
id: BR-FICHE-030
domaine: FICHE
titre: Après l'installation, un réglage n'est modifiable que si la fiche le déclare ; un réglage figé à la création du monde ne se change pas
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-030), HRT-101
maj: 2026-11-16
---

# BR-FICHE-030

## Règle
La description garde, pour chaque réponse, si la question est figée à la création du monde (`frozen`). La modification d'un réglage (onglet « Réglages ») est portée par HRT-108 et HRT-109. Un réglage reçu par variable ou argument s'enregistre aussi : les réponses de la description sont révisées par `apply_game_env` (une réponse figée est refusée `frozen`, une valeur secrète reste scellée avec les secrets, jamais dans le corps), puis le service est recréé (HRT-266, BR-FICHE-042).

## Application (code)
- `crates/hearth-agent/src/domain/services/spec.rs::SpecAnswer`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::body_and_secrets_from`
- `crates/hearth-agent/src/domain/services/update.rs::apply_game_env`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::the_fiche_description_keeps_answers_and_frozen_questions`

## Cas limites
- Une réponse figée à l'installation n'est jamais changée par la recréation (`frozen`, BR-EDIT-035).

## Règles liées
- BR-FICHE-029, BR-EDIT-035

## Historique
- 2026-11-16 : un réglage de variable ou d'argument se change par la recréation (HRT-266).
- 2026-10-24 — création (HRT-101).
