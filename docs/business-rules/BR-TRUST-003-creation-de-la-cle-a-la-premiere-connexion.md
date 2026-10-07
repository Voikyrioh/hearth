---
id: BR-TRUST-003
domaine: TRUST
titre: À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil, gardée dans le coffre de Windows
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-003 : À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil, gardée dans le coffre de Windows

## Règle
À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil (Ed25519). La partie secrète est conservée dans le coffre de Windows, sans accès direct ; le serveur ne reçoit que la partie publique, avec la preuve qu'on possède la partie secrète. Une clé par serveur du carnet (ADR-0023).

> Les deux moitiés sont livrées : l'agent (HRT-22) accepte, vérifie et inscrit la clé ; le client (HRT-23) la crée, la garde et la présente.

## Application (code)
- Agent : `crates/hearth-agent/src/application/trust.rs::TrustService::on_login` (la clé présentée est inscrite dans la transaction de la connexion accordée).
- Client : `crates/hearth-link/src/adapters/device_key.rs::DeviceKey::generate` (clé Ed25519 par `ring`, opaque), `ports/vault.rs::SecretKind::DeviceKey` (coffre `Hearth/{id}/device-key`), `manager/device.rs::login` (défi, preuve, création, écrite au coffre seulement après le `201` et si l'agent a pris la clé en compte), `manager/mod.rs::{add_and_login, login, remove_server}` ; coquille : `apps/desktop/src-tauri/src/vault.rs::credential_target`.

## Vérification
- `tests/device_proof.rs::a_valid_proof_with_the_right_password_enrolls_the_device_and_learns_its_address`.
- `tests/device_http.rs::a_login_with_a_proof_enrolls_and_a_login_without_one_answers_as_before` (la connexion sans clé répond exactement comme avant).
- Client, contre un vrai agent : `crates/hearth-link/tests/device_key.rs::{the_first_password_login_creates_the_key_and_the_agent_enrolls_this_pc, a_refused_connection_leaves_no_key_behind, one_key_per_server_and_removing_a_server_removes_its_key, a_challenge_the_agent_never_issued_connects_and_keeps_no_key, a_vault_that_fails_for_the_key_never_stops_the_connection}` ; `adapters::device_key::tests` ; coquille : `apps/desktop/src-tauri/tests/devices_runtime.rs::the_key_is_created_at_the_first_login_and_the_list_says_this_pc_without_any_key`, `tests/vault.rs::the_device_key_lives_under_hearth_slash_id_slash_device_key_and_goes_with_its_server`.

## Cas limites
- Une connexion refusée n'inscrit rien (BR-TRUST-004) : le client jette la paire créée, rien n'est écrit au coffre.
- Un client qui n'envoie pas de clé (le client actuel) se connecte comme avant : l'agent n'exige jamais de clé.

## Règles liées
- BR-TRUST-004, BR-TRUST-005, BR-TRUST-026, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
