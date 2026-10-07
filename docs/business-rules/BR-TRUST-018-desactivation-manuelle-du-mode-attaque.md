---
id: BR-TRUST-018
domaine: TRUST
titre: Désactiver le mode attaque à la main depuis le client : administrateur, mot de passe actuel, preuve de clé du poste, comme l'activation
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-018) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-018 : Désactiver le mode attaque à la main depuis le client : administrateur, mot de passe actuel, preuve de clé du poste, comme l'activation

## Règle
`PUT /security/attack-mode {"active": false, "password", "device"}` : mêmes conditions que l'activation (BR-TRUST-028). La preuve (usage `0x03`) est liée au jeton de la session ET au geste : une preuve d'activation ne désactive pas, et inversement. Désactiver pendant la fenêtre de redémarrage est possible et le mode n'est pas rouvert après (BR-TRUST-020). Idempotent : désactiver un mode éteint ne change rien et n'écrit rien. Consigné `attack_mode.disable` (BR-TRUST-030), `last_end: "manual"`.

Sur le serveur lui-même : `hearth-agent attack-mode off` (BR-TRUST-027), `last_end: "cli"`.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/security.rs::set_attack_mode`, `crates/hearth-agent/src/application/sessions.rs::SessionService::set_attack_mode`.
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::change`.

## Vérification
- `attack_mode_http.rs` : `an_administrator_with_a_proved_key_and_the_password_enables_then_disables_the_mode`, `every_kind_of_wrong_proof_is_refused_with_409_and_leaves_the_mode_off` (une preuve d'activation ne désactive pas, et inversement).
- `attack_mode.rs` : `a_disable_during_the_window_is_not_reopened_after_it`, `lockout_exit_2_the_local_command_ends_the_mode_without_the_network`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-010, 020, 027, 028, 030, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
- 2026-10-07 : côté client (HRT-26, T38) : le bouton « Désactiver le mode attaque » de la carte, même confirmation avec mot de passe, mêmes raisons d'indisponibilité que l'activation (BR-TRUST-010, 029) ; message « Mode attaque désactivé. » ; `src/pages/SecurityMode.test.ts` (« deactivates with the same confirmation »), `apps/desktop/src-tauri/tests/security_runtime.rs::an_administrator_with_the_key_activates_then_deactivates_with_the_password`.
- 2026-10-07 : HRT-28 : la désactivation accepte aussi le contrat `reauth` (`0x05`), même règle de clé (BR-TRUST-041) ; une activation ferme toutes les élévations (BR-TRUST-043).
