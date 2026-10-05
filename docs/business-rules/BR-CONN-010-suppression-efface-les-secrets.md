---
id: BR-CONN-010
domaine: CONN
titre: Supprimer un serveur efface aussi ses identifiants mémorisés du coffre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-010)
maj: 2026-10-05
---

# BR-CONN-010 — Supprimer un serveur efface aussi ses identifiants mémorisés du coffre

## Règle
`remove_server` arrête la tâche du serveur puis efface le jeton et le mot de passe mémorisé du coffre, la dernière vue, les opérations en suspens et l'entrée du carnet. C'est la seule action qui efface le mot de passe mémorisé de façon voulue (la déconnexion le garde, BR-CONN-016).

## Application (code)
- `crates/hearth-link/src/manager/mod.rs::LinkManager::remove_server`.

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::servers_are_validated_listed_and_removed_with_their_secrets`.

## Cas limites
- Un serveur inconnu rend `UnknownServer`, rien n'est effacé ailleurs.

## Règles liées
- BR-CONN-016.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
