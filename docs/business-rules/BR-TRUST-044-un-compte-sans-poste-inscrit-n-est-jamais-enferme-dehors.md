---
id: BR-TRUST-044
domaine: TRUST
titre: Un compte sans poste inscrit n'est jamais enfermé dehors : reconnexion par mot de passe ou commandes du serveur ; aucune voie par session seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-07-technique-administration-mot-de-passe-et-cle.md ; contexts/hearth/tickets/hrt/HRT-30.md
maj: 2026-10-07
---

# BR-TRUST-044 : Un compte sans poste inscrit n'est jamais enfermé dehors

## Règle
- Un poste ne s'inscrit que par une connexion par mot de passe accordée (ADR-0023) ; aucune route n'inscrit une clé par session seule. Quand l'agent EXIGE la confirmation (BR-TRUST-045), un administrateur sans poste inscrit retrouve la main par :
  1. **première connexion après l'installation** : connexion par mot de passe avec preuve, poste inscrit dans la transaction, actes possibles aussitôt ;
  2. **clé perdue** (nouveau PC, coffre vidé) : connexion par mot de passe, nouvelle clé inscrite ; le client le propose (« Me reconnecter pour enregistrer ce poste ») ;
  3. **compte à 8 postes** (`device: "limit"`) : `hearth-agent account revoke <compte>` (oublie les postes et les adresses), puis la connexion inscrit ;
  4. **mode attaque actif, pas de clé** (inscription gelée) : `hearth-agent attack-mode off`, puis la connexion inscrit ;
  5. **plus aucun accès par le client** : chaque acte a son équivalent sur le serveur (`account add|passwd|role|remove|revoke`, `attack-mode off`, réinstallation pour la mise à jour de l'agent).
- Aucun repli : sur un agent qui exige, `reauth` absent ou sans preuve ne retombe jamais sur « session et rôle ».

## Application (code)
- `entrypoint/http/reauth.rs::layer` (aucun repli) ; `entrypoint/cli.rs` (`account`, `attack-mode`) ; `application/accounts.rs::revoke_sessions`.

## Vérification
- `crates/hearth-agent/tests/admin_reauth.rs::{rescue_first_login_after_the_install_enrolls_the_device_and_the_first_act_passes, rescue_a_lost_key_is_replaced_by_a_login_with_the_password_and_the_act_passes, rescue_an_account_at_eight_devices_is_unlocked_by_the_server_command_then_a_login, rescue_with_the_attack_mode_on_and_no_key_the_server_command_then_a_login_enrolls}` ; client : `crates/hearth-link/tests/admin_reauth.rs::without_a_key_no_call_leaves_not_even_a_challenge`.

## Règles liées
- BR-TRUST-036, 045, 049, ADR-0023, ADR-0033, `docs/runbooks/recuperer-acces-administrateur.md`.

## Historique
- 2026-10-07 : création (HRT-30, tranche C).
