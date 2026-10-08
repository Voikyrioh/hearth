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
- **L'exigence est le défaut du service, et il n'existe aucun moyen, même en test, d'agir sans confirmation** (HRT-18 tranche 7) : ni drapeau, ni méthode de réglage, ni marqueur `unconfirmed`, ni fonction cargo `test-support` côté agent, ni bras « accepte » dans la couche. Tous les bancs (agent, liaison, coquille) envoient des actes CONFIRMÉS (aide `Actor` / `Api::act` côté agent, `Options::with_device_key()` + `execute_act` côté liaison). Le handler de CHAQUE route d'acte prend `Extension<Reauthenticated>` dans sa signature, jamais optionnelle. `tests/confirmation_guard.rs` fait échouer la CI si l'un de ces moyens réapparaît dans `src/` ou dans un manifeste du workspace, ou si un handler d'acte ne prend pas l'extension. Reste côté client `LinkManager::execute_raw` (fonction `test-support` de `hearth-link`, BR-TRUST-050) : il peut atteindre un vrai agent (`admin_reauth.rs`, `robustness.rs`), qui répond `426` : ce n'est pas un moyen d'agir, et il est hors du client publié.

## Application (code)
- `entrypoint/http/reauth.rs::{layer, too_old}` ; `application/sessions.rs::reauthenticate` ; garde : `tests/confirmation_guard.rs`.

## Vérification
- `tests/admin_reauth.rs::{when_the_agent_requires_the_confirmation_an_act_without_reauth_is_told_to_update_the_client, the_setting_is_per_account_any_role_and_always_confirmed}`.

## Règles liées
- BR-TRUST-036, 046, ADR-0031.

## Historique
- 2026-10-07 : création (HRT-28).
- 2026-10-07 (HRT-30, tranche C) : **l'agent exige.** `SessionService::new` exige (le démarrage n'a plus rien à régler) ; `GET /security` annonce `required: true`. Un client ancien garde sa connexion, son flux et ses lectures et reçoit `426` (`LinkFailure::IncompatibleClient`, « client trop ancien ») sur ses actions, jamais une erreur générique ; le refus est consigné sans secret. (La tolérance de la forme à plat `0x03` du mode attaque est tombée en HRT-18 tranche 5.) **Ordre de déploiement : le client d'abord, l'agent ensuite.** Les bancs de test qui envoient des actes bruts appellent `accept_unconfirmed_acts_for_tests(true)`.- 2026-10-08 (HRT-18 tranche 4) : les bancs de l'agent envoient des actes confirmés (aide `Actor`/`Api::act`, `Agent::confirmed`) ; la porte n'est plus demandée par l'agent pour lui-même, seulement par `hearth-link` et la coquille. La garde de texte porte sur l'écriture du drapeau `reauth_required` et la garde de manifestes lit tous les membres du workspace. (La forme à plat `0x03` annoncée gardée ici est retirée en tranche 5.)
- 2026-10-08 (HRT-18 tranche 3) : la porte des bancs d'essai passe derrière la fonction cargo `test-support` ; phrase « aucun chemin de composition ne peut servir HTTP sans le demander » désormais vraie sans condition.
- 2026-10-08 (HRT-18 tranche 7) : porte des bancs d'essai fermée. Les 15 derniers usagers (`fault_proxy` 9, `session_end_actions` 5, `agent_update` 1) passent par `execute_act` ; la pause du proxy sait geler à partir de la n-ième connexion (`freeze_from`) pour que l'acte seul parte dans le vide. `accept_unconfirmed_acts_for_tests`, `Options::accepting_bare_acts`, `Reauthenticated::unconfirmed`, `reauth_required`, la fonction cargo de l'agent et `bench_guard.rs` sont supprimés.
