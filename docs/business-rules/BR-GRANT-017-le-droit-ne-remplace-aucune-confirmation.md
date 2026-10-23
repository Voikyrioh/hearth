---
id: BR-GRANT-017
domaine: GRANT
titre: Le droit ne remplace aucune confirmation : la preuve de clé à chaque acte, le mot de passe selon la grille de confiance
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-017), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md (section 13), HRT-194
maj: 2026-10-23
---

# BR-GRANT-017 : Le droit ne remplace aucune confirmation : la preuve de clé à chaque acte, le mot de passe selon la grille de confiance

## Règle
Le droit « gérer les services » dit si le compte peut DEMANDER l'acte ; la confirmation dit que c'est bien lui. Un compte qui a le droit passe toujours par la confirmation du socle pour chaque acte : la preuve de la clé du poste dans tous les états de confiance, et le mot de passe selon `hearth_proto::admin_act::password_need(act, trust)` :

- **actes de pilotage** (démarrer, arrêter, redémarrer, répondre à la décision d'une opération ; `is_piloting`, BR-LIFE-016) : NORMAL aucun mot de passe, ALERTE le mot de passe puis l'élévation de 5 minutes (réglage « à chaque action » : à chaque acte), MODE ATTAQUE (actif ou suspendu) le mot de passe à chaque acte ;
- **tous les autres actes** (supprimer un service, restaurer ou écraser un monde, accorder le droit, déployer, modifier un service, adopter, annuler une opération...) : la table de l'élévation (`covered_by_elevation`), inchangée quel que soit l'état de confiance. Accorder le droit reste un acte irréversible : le mot de passe dans tous les états (il n'est aujourd'hui possible qu'en ligne de commande, en root, BR-GRANT-008).

Un acte qui arrive à `password_need` doit être classé : la correspondance de `is_piloting` est exhaustive, sans `_`.

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{password_need, is_piloting, covered_by_elevation}`
- `crates/hearth-agent/src/application/sessions.rs::reauthenticate`

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests::{the_password_need_follows_the_trust_grid_for_piloting_and_the_elevation_table_for_the_rest, what_is_not_piloting_keeps_its_password_in_every_state}`
- `crates/hearth-agent/tests/admin_reauth.rs` (matrice par état : `in_the_normal_state_piloting_needs_no_password_opens_no_elevation_and_ignores_the_each_setting`, `the_acts_the_elevation_never_covers_ask_for_the_password_during_the_elevation`, `an_account_without_the_services_right_cannot_cancel_or_decide_even_with_a_valid_confirmation`)

## Règles liées
- BR-LIFE-016, BR-OPS-003, BR-TRUST-042, BR-TRUST-043, BR-TRUST-052, BR-DEPLOY-015, BR-ADOPT-008, BR-GRANT-008

## Historique
- 2026-10-23 : création de la fiche du dépôt (HRT-194). Amendée 2026-10-20 (décision Voiky) : pour piloter un service, le mot de passe suit l'état de confiance ; accorder le droit garde le mot de passe dans tous les états.
