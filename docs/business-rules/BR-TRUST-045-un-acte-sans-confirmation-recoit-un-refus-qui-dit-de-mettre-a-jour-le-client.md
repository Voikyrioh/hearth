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
- Le mode attaque n'a plus de forme à plat : sans `reauth`, sa route répond `426` comme les autres actes. `PUT /me/reauth` est toujours exigé, même quand l'agent n'exige pas.
- **L'exigence est le défaut du service** (`SessionService::new`). Le régime « la session suffit » n'existe que pour des bancs d'essai de `hearth-link`, et **le banc exige par défaut** (HRT-18 tranche 5, `Options::bare_acts: false`) : seuls les scénarios qui envoient volontairement une route d'acte brute (`execute_raw`) à un vrai agent le baissent, par `Options::accepting_bare_acts()` : 9 de `fault_proxy.rs`, les 5 de `session_end_actions.rs` (un seul appel) et 1 de `agent_update.rs` (un autre administrateur lance la mise à jour par une session brute). Les 11 de `tracking.rs` n'ont pas besoin de la porte (transport simulé, pas d'agent). `tests/bench_guard.rs` ferme la liste des fichiers autorisés et leur nombre d'appels : un nouvel usager fait échouer la garde. Aucun banc de l'agent n'y recourt. La porte `accept_unconfirmed_acts_for_tests` est derrière la fonction cargo `test-support` (demandée par les seuls dev-dependencies), donc **absente d'un binaire de production** ; `tests/test_support_guard.rs` (agent) fait échouer la CI si le nom apparaît dans `src/` hors de sa définition gardée.

## Application (code)
- `entrypoint/http/reauth.rs::{layer, too_old}` ; `application/sessions.rs::{reauth_required, accept_unconfirmed_acts_for_tests}` (sous `test-support`) ; garde : `tests/test_support_guard.rs`.

## Vérification
- `tests/admin_reauth.rs::{when_the_agent_requires_the_confirmation_an_act_without_reauth_is_told_to_update_the_client, the_setting_is_per_account_any_role_and_always_confirmed}`.

## Règles liées
- BR-TRUST-036, 046, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28).
- 2026-10-07 (HRT-30, tranche C) : **l'agent exige.** `SessionService::new` exige (le démarrage n'a plus rien à régler) ; `GET /security` annonce `required: true`. Un client ancien garde sa connexion, son flux et ses lectures et reçoit `426` (`LinkFailure::IncompatibleClient`, « client trop ancien ») sur ses actions, jamais une erreur générique ; le refus est consigné sans secret. (La tolérance de la forme à plat `0x03` du mode attaque est tombée en HRT-18 tranche 5.) **Ordre de déploiement : le client d'abord, l'agent ensuite.** Les bancs de test qui envoient des actes bruts appellent `accept_unconfirmed_acts_for_tests(true)`.- 2026-10-08 (HRT-18 tranche 4) : les bancs de l'agent envoient des actes confirmés (aide `Actor`/`Api::act`, `Agent::confirmed`) ; la porte n'est plus demandée par l'agent pour lui-même, seulement par `hearth-link` et la coquille. La garde de texte porte sur l'écriture du drapeau `reauth_required` et la garde de manifestes lit tous les membres du workspace. (La forme à plat `0x03` annoncée gardée ici est retirée en tranche 5.)
- 2026-10-08 (HRT-18 tranche 3) : la porte des bancs d'essai passe derrière la fonction cargo `test-support` ; phrase « aucun chemin de composition ne peut servir HTTP sans le demander » désormais vraie sans condition.
