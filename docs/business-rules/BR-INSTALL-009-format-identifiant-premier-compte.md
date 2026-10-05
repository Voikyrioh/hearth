---
id: BR-INSTALL-009
domaine: INSTALL
titre: Format de l'identifiant du premier compte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-009), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-009 : Format de l'identifiant du premier compte

## Règle
3 à 32 caractères : lettres minuscules, chiffres, tirets, underscores. La saisie est ramenée en minuscules. Vide : « Le nom du compte est requis. ». Autre refus : « Le nom du compte contient des caractères non autorisés. » (suivi du texte d'aide).

## Application (code)
- `crates/hearth-agent/src/domain/install/credentials.rs::parse_admin_name`, qui s'appuie sur `domain::accounts::Username::parse` (BR-ACCT-002).

## Vérification
- `domain::install::credentials::tests`.
- `tests/install_flow.rs::invalid_entries_are_explained_and_asked_again_without_leaving`.

## Cas limites
- Un nom trop court ou trop long reçoit le message des caractères non autorisés (la spécification n'en donne qu'un) ; le texte d'aide dit la longueur attendue.

## Règles liées
- BR-ACCT-002

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
