---
id: BR-TRUST-043
domaine: TRUST
titre: L'élévation vit dans l'agent, dure 5 minutes non glissantes, est liée à la session, au poste et à l'adresse, se ferme sur les causes listées, n'existe pas en mode attaque et ne couvre jamais les actes exclus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-28.md
maj: 2026-10-07
---

# BR-TRUST-043 : L'élévation vit dans l'agent, dure 5 minutes non glissantes, est liée à la session, au poste et à l'adresse, se ferme sur les causes listées, n'existe pas en mode attaque et ne couvre jamais les actes exclus

## Règle
- Après un mot de passe juste à une confirmation (réglage `window`, mode attaque éteint), le mot de passe n'est plus demandé pendant 5 minutes **pour les actes couverts** ; la preuve de clé reste exigée. Mémoire de l'agent, horloge monotone, une entrée par session (bornée à 1 024), jamais en base ni côté client.
- **Non glissante** : un acte ne la prolonge pas ; seule une nouvelle saisie du mot de passe la relance.
- **Couverts** : créer un compte en lecture seule, passer un compte en lecture seule, supprimer un compte, fermer les sessions d'un compte. **Jamais couverts** : retirer un poste (contrat `0x04`), activer ou désactiver le mode attaque, changer un mot de passe (le sien ou celui d'un autre), donner le rôle Administrateur (changement de rôle ou création), lancer la mise à jour de l'agent, changer le réglage.
- **Fermée par** : l'échéance ; la fin de la session ; le redémarrage du service ; le changement du mot de passe ou du rôle du compte (ou sa suppression, ou la fermeture de ses sessions) ; le retrait ou l'oubli du poste ; le mode attaque actif ou suspendu (aucune n'existe tant qu'il dure) ; le passage du réglage à `each` ; le premier mot de passe faux du compte à une confirmation ; une autre adresse, une autre session ou un autre poste ne sont pas couverts.
- Refus quand elle n'est pas valable et que le mot de passe manque : `409 POST_NOT_RECOGNIZED`, `reason: password_required`.

## Application (code)
- `domain/trust/admin_act.rs::{covered_by_elevation, elevation_holds, elevation_remaining_s, ELEVATION_MS}` ; `application/elevation.rs::Elevations` ; `application/sessions.rs::reauthenticate`.

## Vérification
- `tests/admin_reauth.rs` : `a_right_password_opens_five_minutes_where_a_covered_act_needs_only_the_proof_and_a_use_never_extends_it`, `the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation`, `changing_your_own_password_asks_for_the_password_during_the_elevation_and_closes_it`, et un test par cause de fermeture (`the_elevation_ends_with_the_session`, `the_elevation_lives_in_memory_only_so_a_restart_of_the_service_starts_without_one`, `changing_the_password_or_the_role_of_the_account_closes_the_elevation`, `removing_the_device_of_the_elevation_closes_it`, `the_attack_mode_closes_every_elevation_and_none_exists_while_it_lasts`, `setting_each_closes_the_elevation_and_asks_the_password_for_every_act`, `the_first_wrong_password_of_the_account_at_a_confirmation_closes_the_elevation`, `a_wrong_password_on_a_flat_form_closes_the_elevation_too`, `deleting_the_account_or_closing_its_sessions_closes_its_elevations`, `the_flat_attack_mode_closes_the_elevations_when_it_turns_on`, `the_elevation_is_not_valid_for_another_session_another_device_or_another_address`, `setting_the_wall_clock_back_or_forward_never_moves_the_five_minutes`) ; unitaires `domain::trust::admin_act::tests`, `application::elevation::tests`.

## Règles liées
- BR-TRUST-042, 047, ADR-0032.

## Historique
- 2026-10-07 : création (HRT-28, tranche D1 ; décision Q19 ; liste des actes exclus : choix de Claude à remontrer à Voiky).
- 2026-10-07 (HRT-30) : la règle de couverture `covered_by_elevation` est déplacée dans `hearth_proto::admin_act` (source unique lue par l'agent, qui décide, et par le client, qui sait s'il faut demander le mot de passe) ; `domain/trust/admin_act.rs::covered_by_elevation` la délègue. Le client lit l'élévation de l'agent à chaque ouverture de fenêtre (BR-TRUST-052).