---
id: BR-FICHE-017
domaine: FICHE
titre: Une réponse texte ne contient ni retour à la ligne ni caractère que le format du jeu interprète ; le refus cite le caractère, jamais la valeur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-017), HRT-101
maj: 2026-10-24
---

# BR-FICHE-017

## Règle
Une réponse de type texte reste dans le jeu de caractères de son réglage (`line`, `file_stem`, alphabets fermés), sans retour à la ligne ni caractère de contrôle, dans sa longueur maximale ; le nom d'un monde, qui nomme des fichiers, est plus strict. Un secret n'est pas jugé par jeu de caractères mais par sa longueur et la forme du fichier où il est écrit. Le refus (`SETTING_REFUSED`, `details { id, reason, char }`) cite au plus UN caractère, jamais la valeur reçue ni, pour un identifiant inconnu de la fiche, l'identifiant reçu.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::validate_text`
- `crates/hearth-agent/src/domain/catalog/loading/value.rs::first_refused_char`
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::an_invalid_answer_is_refused_naming_the_character_never_the_value`

## Règles liées
- BR-FICHE-016, BR-FICHE-007

## Historique
- 2026-10-24 — création (HRT-101).
