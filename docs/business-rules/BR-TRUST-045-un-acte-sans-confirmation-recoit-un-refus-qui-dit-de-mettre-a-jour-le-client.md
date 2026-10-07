---
id: BR-TRUST-045
domaine: TRUST
titre: Un acte sans confirmation reçoit un refus qui dit de mettre le client à jour ; aucun repli vers la session seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-045 : Un acte sans confirmation reçoit un refus qui dit de mettre le client à jour ; aucun repli vers la session seule

## Règle
- Quand l'agent **exige** la confirmation (`admin_reauth.required: true`), un acte sans membre `reauth` reçoit `426 INCOMPATIBLE_VERSION` (`details.upgrade: "client"`, `details.reason: "reauth_required"`), consigné « confirmation absente : client trop ancien » ; rien n'est fait, aucun mot de passe n'est essayé. Un `reauth` présent sans preuve est autre chose : `409 proof_missing` (client récent sans clé).
- Le mode attaque garde sa forme à plat (`0x03`) : sans `reauth`, sa route répond comme avant (preuve et mot de passe exigés par le handler). `PUT /me/reauth` est toujours exigé, même quand l'agent n'exige pas.
- **L'exigence est le défaut du service** (`SessionService::new`) : aucun chemin de composition ne peut servir HTTP en « la session suffit » sans le demander par le nom explicite `accept_unconfirmed_acts_for_tests` (bancs d'essai).

## Application (code)
- `entrypoint/http/reauth.rs::{layer, too_old}` ; `application/sessions.rs::{reauth_required, accept_unconfirmed_acts_for_tests}`.

## Vérification
- `tests/admin_reauth.rs::{when_the_agent_requires_the_confirmation_an_act_without_reauth_is_told_to_update_the_client, the_setting_is_per_account_any_role_and_always_confirmed}`.

## Règles liées
- BR-TRUST-036, 046, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28).
- 2026-10-07 (HRT-30, tranche C) : **l'agent exige.** `SessionService::new` exige (le démarrage n'a plus rien à régler) ; `GET /security` annonce `required: true`. Un client ancien garde sa connexion, son flux et ses lectures et reçoit `426` (`LinkFailure::IncompatibleClient`, « client trop ancien ») sur ses actions, jamais une erreur générique ; le refus est consigné sans secret. Le mode attaque garde la tolérance de la forme à plat `0x03` (clients livrés) ; elle tombe à la première hausse de `API_MIN_SUPPORTED` (ADR-0033). **Ordre de déploiement : le client d'abord, l'agent ensuite.** Les bancs de test qui envoient des actes bruts appellent `accept_unconfirmed_acts_for_tests(true)`.