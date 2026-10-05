---
id: BR-INSTALL-010
domaine: INSTALL
titre: Format du mot de passe du premier compte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-010), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-010 : Format du mot de passe du premier compte

## Règle
12 caractères au moins, une majuscule, une minuscule, un chiffre, et pas l'identifiant. Vide : « Le mot de passe est requis. ». Autre refus : « Le mot de passe est trop court. » (suivi du texte d'aide). Saisi sans écho, avec confirmation.

## Application (code)
- `crates/hearth-agent/src/domain/install/credentials.rs::check_admin_password`, qui s'appuie sur `domain::accounts::password::unmet_rules` (BR-ACCT-004 et 005).

## Vérification
- `domain::install::credentials::tests`.
- `tests/install_flow.rs` : `a_bad_variable_is_refused_before_any_write`, `invalid_entries_are_explained_and_asked_again_without_leaving`.

## Cas limites
- Le mot de passe n'apparaît ni dans un message, ni dans un `Debug` (`Secret`), ni dans une ligne de commande, ni dans le fichier d'unité.

## Règles liées
- BR-ACCT-004, BR-ACCT-006

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
