---
id: BR-ACCT-012
domaine: ACCT
titre: Supprimer son propre compte demande de retaper son identifiant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-012), HRT-03
maj: 2026-10-06
---

# BR-ACCT-012 — Supprimer son propre compte demande de retaper son identifiant

## Règle
Un administrateur qui supprime son propre compte doit retaper son identifiant (insensible à la casse, espaces autour ignorés). Sinon : « L'identifiant ne correspond pas, réessaye » et rien n'est supprimé. S'ajoute à la garde du dernier administrateur (BR-ACCT-007).

## Application (code)
- `crates/hearth-agent/src/domain/accounts/self_deletion.rs::confirm_self_deletion`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete` — paramètres `acting` (compte appelant) et `confirmation` ; la confirmation n'est exigée que si `acting` est la cible. La ligne de commande n'a pas d'appelant et n'est donc pas concernée.
- Route : `DELETE /api/v1/accounts/{id}` avec `{ "confirmation": "<identifiant>" }` (`entrypoint/http/accounts.rs::delete`) ; refus `VALIDATION_ERROR` (`details.field = "confirmation"`).

## Interface (HRT-13)
« Supprimer mon compte » (section « Mon compte » des réglages, administrateurs) → `FormDialog` « Supprimer ton compte ? » : champ « Retape ton identifiant pour confirmer » ; l'interface n'évalue pas la correspondance, l'agent compare (`DELETE /accounts/{id}` avec `confirmation`) : « L'identifiant ne correspond pas, réessaye ». Au succès, le mot de passe mémorisé du compte disparu est oublié et le lien passe à « Accès révoqué ». Test : `accounts_runtime.rs::deleting_your_own_account_asks_for_your_username_and_ends_your_session`.
« Mon compte » est trouvé par l'identifiant de l'agent de la session (`list_accounts` rend `me`), jamais par comparaison de texte avec la saisie de connexion.

## Vérification
- Tests : `domain::accounts::self_deletion::tests` ; `crates/hearth-agent/tests/accounts_use_cases.rs::deleting_your_own_account_requires_your_username`.

## Cas limites
- Message au tutoiement (« réessaye ») ; le tableau des messages de la spec disait « réessayez », à reporter dans la spec.

## Règles liées
- BR-ACCT-007.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
