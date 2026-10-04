---
id: BR-ACCT-013
domaine: ACCT
titre: Un compte lecture seule n'a pas accès à la gestion des comptes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-013), HRT-03
maj: 2026-10-04
---

# BR-ACCT-013 — Un compte lecture seule n'a pas accès à la gestion des comptes

## Règle
Un compte lecture seule ne voit pas la gestion des comptes ; il peut uniquement changer son propre mot de passe. La règle « qui peut gérer les comptes » est une fonction pure du rôle, que la garde de HRT-04 appelle. Message : « Tu n'as pas la permission pour accéder à la gestion des comptes ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/role.rs::Role::can_manage_accounts`.
- Garde HTTP : à venir avec HRT-04 (extracteur `AdminOnly`).

## Vérification
- Tests : `domain::accounts::role::tests::only_an_administrator_manages_accounts`.

## Cas limites
- Le changement de son propre mot de passe (BR-ACCT-009) reste permis à tout rôle.

## Règles liées
- BR-ACCT-014.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
