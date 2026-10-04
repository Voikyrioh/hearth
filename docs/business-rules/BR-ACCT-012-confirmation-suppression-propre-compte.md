---
id: BR-ACCT-012
domaine: ACCT
titre: Supprimer son propre compte demande de retaper son identifiant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-012), HRT-03
maj: 2026-10-04
---

# BR-ACCT-012 — Supprimer son propre compte demande de retaper son identifiant

## Règle
Un administrateur qui supprime son propre compte doit retaper son identifiant (insensible à la casse, espaces autour ignorés). Sinon : « L'identifiant ne correspond pas, réessaye » et rien n'est supprimé. S'ajoute à la garde du dernier administrateur (BR-ACCT-007).

## Application (code)
- `crates/hearth-agent/src/domain/accounts/self_deletion.rs::confirm_self_deletion`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete` — paramètres `acting` (compte appelant) et `confirmation` ; la confirmation n'est exigée que si `acting` est la cible. La ligne de commande n'a pas d'appelant et n'est donc pas concernée.

## Vérification
- Tests : `domain::accounts::self_deletion::tests` ; `tests/accounts_use_cases.rs::deleting_your_own_account_requires_your_username`.

## Cas limites
- La spec propose « réessayez » dans le tableau des messages et « réessaye » dans les comportements ; le tutoiement est retenu.

## Règles liées
- BR-ACCT-007.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
