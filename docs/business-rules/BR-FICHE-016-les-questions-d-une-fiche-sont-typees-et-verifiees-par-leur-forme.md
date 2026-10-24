---
id: BR-FICHE-016
domaine: FICHE
titre: Chaque question d'une fiche a un type et des bornes ; une réponse est vérifiée par sa forme, jamais interprétée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-016), HRT-101
maj: 2026-10-24
---

# BR-FICHE-016

## Règle
Une question est un réglage essentiel du catalogue de la fiche (texte, secret, entier, choix, booléen, liste d'identifiants). Une réponse est refusée hors de son type et de ses bornes (`Bounds`, `Choice`, `Length`, `Charset`), jamais interprétée : elle est vérifiée par sa forme puis écrite dans la configuration du jeu sans autre transformation que celle que la fiche décrit. Un réglage figé à la création du monde porte « Ne pourra plus être changé après la création du monde ».

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{validate_text, resolve_install}`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs (réponses refusées par type et bornes)`
- `crates/hearth-agent/tests/fiche_install_https.rs::an_invalid_answer_is_refused_naming_the_character_never_the_value`

## Cas limites
- Les types « jeton de serveur » et « mot de passe de console » relèvent de HRT-120.

## Règles liées
- BR-FICHE-017, BR-FICHE-038

## Historique
- 2026-10-24 — création (HRT-101).
