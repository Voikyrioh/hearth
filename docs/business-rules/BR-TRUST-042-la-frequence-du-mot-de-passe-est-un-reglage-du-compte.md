---
id: BR-TRUST-042
domaine: TRUST
titre: La fréquence du mot de passe est un réglage par compte tenu par l'agent : une saisie pour 5 minutes par défaut (Q19), « à chaque action » offert ; la preuve de clé est donnée à chaque acte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-042 : La fréquence du mot de passe est un réglage par compte tenu par l'agent : une saisie pour 5 minutes par défaut (Q19), « à chaque action » offert ; la preuve de clé est donnée à chaque acte

## Règle
- Réglage `window` (défaut : une saisie du mot de passe ouvre 5 minutes) ou `each` (à chaque action), colonne `accounts.reauth_window_s` (migration 0007 : 300 ou 0, les comptes existants prennent 300).
- Changé par `PUT /me/reauth` (tout rôle), **toujours** avec mot de passe et clé (jamais couvert par l'élévation), journal `reauth.setting`. `GET /security` le rend (`admin_reauth.password`) avec le temps restant de l'élévation (`elevated_for_s`).
- Dans les deux réglages la preuve de clé est exigée à chaque acte. Valeur inconnue en base : lue `each` (le plus strict).

## Application (code)
- `domain/trust/admin_act.rs::{mode_from_seconds, seconds_of}` ; `application/sessions.rs::{set_reauth_mode, admin_reauth_info}` ; `infrastructure/sqlite/account_repo.rs` ; `migrations/0007_reauth_window.sql`.

## Vérification
- `tests/admin_reauth.rs::{the_setting_is_per_account_any_role_and_always_confirmed, setting_each_closes_the_elevation_and_asks_the_password_for_every_act}` ; `tests/migration_0007.rs` ; `domain::trust::admin_act::tests::the_setting_is_read_strictly_and_round_trips`.

## Règles liées
- BR-TRUST-043, ADR-0032.

## Historique
- 2026-10-07 : création (HRT-28, tranche D1 ; décision Q19).
