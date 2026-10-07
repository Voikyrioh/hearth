---
id: BR-TRUST-040
domaine: TRUST
titre: La preuve est vérifiée avant le mot de passe ; le mot de passe passe par le chemin de la connexion et un échec compte comme une connexion ratée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-040 : La preuve est vérifiée avant le mot de passe ; le mot de passe passe par le chemin de la connexion et un échec compte comme une connexion ratée

## Règle
- Ordre : session et rôle, acte reconstruit, preuve de clé (**aucun mot de passe n'est essayé** tant qu'elle n'est pas valable : une session volée, sans clé, ne devine rien), puis le mot de passe par `confirm_password_proven` (tour par adresse, admission, Argon2, compteurs du couple, de l'adresse et de l'identifiant, ralentissement), puis le handler.
- Mot de passe faux : `422 WRONG_PASSWORD`, compté comme un échec de connexion (attente à partir du cinquième échec : `429 TOO_MANY_ATTEMPTS`), consigné sous l'acte. Saturation : `503 BUSY`.
- **L'ancien mot de passe de `PUT /me/password` passe par ces mêmes compteurs dans tous les cas**, avec ou sans `reauth` (correction du constat C3 : il était vérifié hors compteurs, une session volée pouvait deviner sans limite).
- **En mode attaque**, l'ancien mot de passe de `PUT /me/password` sans `reauth` est jugé comme une connexion sans clé : un poste qui n'a qu'un critère (adresse retenue, session sans clé) a droit à UN essai (Q11) ; une faute de frappe le consomme et bloque la confirmation jusqu'à la fin du mode (test `in_attack_mode_a_typo_on_put_me_password_without_reauth_costs_the_single_trial_of_the_address`).
- La confirmation va jusqu'au bout même si le client coupe (tâche détachée) : un échec de mot de passe est toujours compté.

## Application (code)
- `application/sessions.rs::{SessionService::reauthenticate, confirm_password, confirm_password_proven}` ; `entrypoint/http/reauth.rs::layer`.

## Vérification
- `tests/admin_reauth.rs` : `a_confirmation_without_a_proof_or_without_a_password_is_refused_with_its_own_reason_and_tries_no_password`, `a_wrong_password_at_the_confirmation_counts_as_a_login_failure_and_the_wait_comes_at_the_fifth` (pour chaque acte), `the_old_password_of_put_me_password_goes_through_the_login_counters_even_without_reauth`.

## Règles liées
- BR-CONN-006, 007, BR-ACCT-009, BR-TRUST-036, 039, ADR-0022, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28, tranche A).
