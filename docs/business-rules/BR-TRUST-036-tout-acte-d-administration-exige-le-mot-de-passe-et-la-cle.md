---
id: BR-TRUST-036
domaine: TRUST
titre: Tout acte d'administration exige, dans la requête même, le mot de passe actuel de l'appelant et la preuve d'une clé inscrite de son compte ; une session seule ne suffit jamais
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-036 : Tout acte d'administration exige, dans la requête même, le mot de passe actuel de l'appelant et la preuve d'une clé inscrite de son compte ; une session seule ne suffit jamais

## Règle
- Un acte d'administration porte un membre `reauth` (`password`, `device`) dans le corps. La couche de confirmation, posée par le routeur sur chaque route d'acte, vérifie la preuve de clé puis le mot de passe avant le handler.
- **L'agent exige dès la construction du service** (`SessionService::new`, `admin_reauth.required: true`) : une requête sans membre `reauth` reçoit `426 INCOMPATIBLE_VERSION` (`reason: reauth_required`) ; une requête qui en porte un est vérifiée jusqu'au bout. Seuls les bancs d'essai peuvent le baisser, par `accept_unconfirmed_acts_for_tests`. Le mode attaque garde en plus sa forme à plat (usage `0x03`), aussi stricte.
- Le réglage de fréquence (`PUT /me/reauth`) est toujours confirmé, dès la tranche A.
- Le retrait d'un poste garde son contrat livré (`0x04`, champs à plat, clé du poste courant : BR-TRUST-022, BR-TRUST-041).
- Aucun repli : une confirmation présente mais sans preuve valable ne retombe jamais sur « session et rôle ».

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::layer` (couche posée depuis la table) ; `application/sessions.rs::SessionService::reauthenticate` ; `hearth-proto/src/admin_act.rs::{ROUTES, AdminAct}` ; `hearth-proto/src/api/reauth.rs::Reauth`.

## Vérification
- `tests/admin_reauth.rs` : `a_current_client_without_reauth_gets_todays_answers_while_the_agent_does_not_require_it`, `every_admin_act_with_a_valid_confirmation_succeeds_and_leaves_one_success_entry`, `when_the_agent_requires_the_confirmation_an_act_without_reauth_is_told_to_update_the_client`, `the_setting_is_per_account_any_role_and_always_confirmed`.

## Règles liées
- BR-TRUST-037, 039, 040, 041, ADR-0031, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-28, tranche A).
- 2026-10-07 : le client confirme (HRT-30, tranche B) : `hearth-link` `execute_act` ajoute `reauth` (BR-TRUST-049, 050, 051), toutes les fenêtres d'acte passent par `AdminActDialog` (BR-TRUST-052). Le mode attaque passe au contrat commun (`0x05`) face à un agent qui annonce `admin_reauth` ; la forme à plat (`0x03`) reste pour un agent d'avant et est toujours acceptée par l'agent pendant la transition (ADR-0033).