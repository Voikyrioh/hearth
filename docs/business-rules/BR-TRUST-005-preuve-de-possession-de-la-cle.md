---
id: BR-TRUST-005
domaine: TRUST
titre: À chaque connexion, le client prouve qu'il possède la clé privée : défi sans état de 60 secondes, usage unique, signature liée au serveur, à l'identifiant et à l'usage
statut: partielle
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-005 : À chaque connexion, le client prouve qu'il possède la clé privée : défi sans état de 60 secondes, usage unique, signature liée au serveur, à l'identifiant et à l'usage

## Règle
Le client prouve qu'il possède la clé privée correspondant à la clé publique enregistrée, par un **défi-réponse applicatif** (ADR-0023) :

- `POST /sessions/challenge { username, purpose }` rend un défi de 56 octets, **sans lecture en base**, **identique pour tout identifiant** (existant ou non). Il vaut 60 secondes (horloge monotone), pour l'adresse qui l'a demandé et pour l'usage demandé.
- Le client signe (Ed25519) le message `hearth_proto::device_proof::signing_bytes` : lié à l'**empreinte du certificat du serveur épinglé**, à l'**identifiant**, à l'**usage** (connexion, session, mode attaque) et, selon l'usage, au **jeton** et au geste. Une preuve valable pour un usage, un identifiant ou un serveur ne vaut pas pour un autre.
- La preuve voyage dans `POST /sessions` (`device`) et dans le premier message du flux (`auth.device`). Le défi dont la preuve est validée est **consommé** (usage unique).
- La signature est vérifiée **sous la clé fournie**, que le compte existe ou non, que la clé soit connue ou non (aucun oracle). Le code du défi se compare à **temps constant**.
- Une preuve absente, illisible, fausse, expirée ou rejouée n'est **jamais une erreur** : la connexion se déroule comme sans clé.

> **Partielle** : HRT-22 livre le côté agent ; le client (HRT-23) n'envoie pas encore de preuve.

## Application (code)
- `crates/hearth-proto/src/device_proof.rs::{signing_bytes, normalize_identifier, key_id, Binding}` (disposition du message, source unique).
- `crates/hearth-agent/src/domain/trust/challenge.rs::{Challenge, mac_input, check, ConsumedChallenges}`.
- `crates/hearth-agent/src/application/trust.rs::TrustService::{issue_challenge, verify}` ; `infrastructure/crypto.rs::{RingProofVerifier, HmacChallengeCrypto}` (`ring`).
- `crates/hearth-agent/src/entrypoint/http/sessions.rs::{challenge, login}` ; `entrypoint/ws/connection.rs::authenticate`.

## Vérification
- Proto : `device_proof::tests` (disposition à la main, un octet de chaque élément lié), `api::sessions::tests`, `stream::tests::the_first_message_reads_with_or_without_a_device_proof`.
- Domaine : `domain::trust::challenge::tests` (expiré, futur, forgé d'un bit, rejoué, ensemble borné).
- `tests/device_proof.rs` : `a_proof_for_one_usage_identifier_server_or_address_is_worth_nothing_for_another`, `an_expired_challenge_is_refused_and_one_at_the_limit_is_not`, `a_replayed_challenge_is_refused_the_proof_serves_once`, `a_forged_or_altered_challenge_is_refused`, `a_signature_by_another_key_or_over_other_bytes_is_refused`, `a_malformed_proof_never_panics_and_is_refused`, `the_challenge_reads_nothing_in_the_database_and_is_the_same_for_any_identifier`, `a_wrong_password_looks_the_same_with_or_without_a_proof_for_an_existing_or_missing_account`, `a_right_password_with_a_false_or_missing_proof_connects_as_if_there_were_no_key`.
- `tests/device_http.rs` : `the_challenge_answers_the_same_for_an_existing_and_a_missing_identifier`, `the_challenge_is_public_versioned_and_strict_about_its_body`, `a_malformed_device_field_never_turns_a_login_into_an_error`, `a_wrong_password_answers_the_same_with_a_valid_proof_a_false_one_or_none`.
- `tests/device_stream.rs` : preuve sous l'empreinte du vrai certificat, de bout en bout (TLS 1.3, WebSocket).

## Cas limites
- Un redémarrage de l'agent invalide les défis en cours : le client en redemande un.
- Un défi dont la preuve est valide mais dont la connexion échoue (mot de passe faux) est consommé : une nouvelle demande est nécessaire.
- Agent d'avant HRT-22 : le défi répond `404` ; le client ne joint rien.
- Lien avec le serveur, pas avec la session TLS elle-même (ADR-0023).

## Règles liées
- BR-TRUST-003, BR-TRUST-004, BR-TRUST-007, BR-CONN-013, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
