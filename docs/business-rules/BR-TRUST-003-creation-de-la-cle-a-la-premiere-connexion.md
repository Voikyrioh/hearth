---
id: BR-TRUST-003
domaine: TRUST
titre: À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil, gardée dans le coffre de Windows
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-003 : À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil, gardée dans le coffre de Windows

## Règle
À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil (Ed25519). La partie secrète est conservée dans le coffre de Windows, sans accès direct ; le serveur ne reçoit que la partie publique, avec la preuve qu'on possède la partie secrète. Une clé par serveur du carnet (ADR-0023).

> **Partielle** : HRT-22 livre la moitié **agent** (accepter, vérifier et inscrire la clé présentée, BR-TRUST-004 et 005). La création et la garde de la clé côté client (`hearth-link`, coffre Windows, `SecretKind::DeviceKey`) sont HRT-23 : tant qu'elles n'existent pas, aucun client n'envoie de clé et la connexion se déroule exactement comme avant.

## Application (code)
- Agent : `crates/hearth-agent/src/application/trust.rs::TrustService::on_login` (la clé présentée est inscrite dans la transaction de la connexion accordée).
- Client (HRT-23, pas encore) : `crates/hearth-link/src/adapters/device_key.rs`, `ports/vault.rs::SecretKind::DeviceKey`.

## Vérification
- `tests/device_proof.rs::a_valid_proof_with_the_right_password_enrolls_the_device_and_learns_its_address`.
- `tests/device_http.rs::a_login_with_a_proof_enrolls_and_a_login_without_one_answers_as_before` (la connexion sans clé répond exactement comme avant).

## Cas limites
- Une connexion refusée n'inscrit rien (BR-TRUST-004) : le client jette la paire créée, rien n'est écrit au coffre.
- Un client qui n'envoie pas de clé (le client actuel) se connecte comme avant : l'agent n'exige jamais de clé.

## Règles liées
- BR-TRUST-004, BR-TRUST-005, BR-TRUST-026, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
