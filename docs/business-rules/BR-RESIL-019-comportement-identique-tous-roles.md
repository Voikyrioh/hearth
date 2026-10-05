---
id: BR-RESIL-019
domaine: RESIL
titre: Le comportement du lien ne dépend pas du rôle du compte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-019)
maj: 2026-10-05
---

# BR-RESIL-019 — Le comportement du lien ne dépend pas du rôle du compte

## Règle
La machine à états, les délais et la résolution des opérations n'ont aucune entrée liée au rôle : un compte « Administrateur » et un compte « Lecture seule » vivent exactement la même résilience. Seules les actions réservées aux administrateurs sont refusées par l'agent (`403 FORBIDDEN_ROLE`), et c'est une réponse comme une autre.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::Input` : aucune variante ne porte de rôle ; `crates/hearth-link/src/domain/pending_ops.rs` idem.

## Vérification
- Vérifié par construction (aucune donnée de rôle dans les types du domaine). Intégration : `tests/fault_proxy.rs::a_read_only_account_has_the_same_link_behaviour`.

## Cas limites
- Une action refusée `403` est une réponse reçue : elle n'est pas « inconnue ».

## Règles liées
- BR-ACCT-013.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
