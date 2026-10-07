---
id: BR-TRUST-005
domaine: TRUST
titre: À chaque connexion, le client prouve qu'il possède la clé privée : défi sans état de 60 secondes, usage unique, signature liée au serveur, à l'identifiant et à l'usage
statut: active
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
- La signature est vérifiée **sous la clé fournie**, que le compte existe ou non, que la clé soit connue ou non (aucun oracle). Le code du défi se compare à **temps constant**. Les clés publiques de **petit ordre** (point neutre et autres points du sous-groupe de torsion, encodages non canoniques compris) sont refusées avant toute vérification : `ring` ne les refuse pas et une signature fixe vérifierait alors n'importe quel message.
- **Le défi n'est consommé que quand la preuve sert** : mot de passe juste ET signature valide (inscription, ou preuve sous une clé inscrite du compte), ou session valide ET signature valide sous une clé inscrite de ce compte. La vérification n'écrit rien : un appareil anonyme n'occupe aucune place (4 096 au plus, 64 par compte). Deux requêtes simultanées avec le même défi : une seule gagne.
- Une preuve absente, illisible, fausse, expirée ou rejouée n'est **jamais une erreur** : la connexion se déroule comme sans clé.

> Les deux côtés sont livrés : l'agent (HRT-22) vérifie, le client (HRT-23) signe.

## Application (code)
- `crates/hearth-proto/src/device_proof.rs::{signing_bytes, normalize_identifier, key_id, Binding}` (disposition du message, source unique).
- `crates/hearth-agent/src/domain/trust/challenge.rs::{Challenge, mac_input, check, ConsumedChallenges}`.
- `crates/hearth-agent/src/application/trust.rs::TrustService::{issue_challenge, verify, consume}` ; `domain/trust/weak_key.rs::has_small_order` ; `infrastructure/crypto.rs::{RingProofVerifier, HmacChallengeCrypto}` (`ring`).
- `crates/hearth-agent/src/entrypoint/http/sessions.rs::{challenge, login}` ; `entrypoint/ws/connection.rs::authenticate`.
- Client : `crates/hearth-link/src/adapters/device_key.rs::DeviceKey::prove` (signe `signing_bytes`, jamais recopié), `manager/device.rs::{request_challenge, session_proof, login, token_hash}` (défi AVANT la connexion et AVANT l'ouverture du flux ; hachage du jeton = SHA-256 des 32 octets du jeton hexadécimal, comme l'agent), `manager/attempt.rs::connect_inner` (premier message `auth` signé), `adapters/http_transport.rs` (`challenge`, `login_with_device`, `send_auth`).

## Vérification
- Proto : `device_proof::tests` (disposition à la main, un octet de chaque élément lié), `api::sessions::tests`, `stream::tests::the_first_message_reads_with_or_without_a_device_proof`.
- Domaine : `domain::trust::challenge::tests` (expiré, futur, forgé d'un bit, rejoué, ensemble borné).
- `tests/device_proof.rs` : `a_proof_for_one_usage_identifier_server_or_address_is_worth_nothing_for_another`, `an_expired_challenge_is_refused_and_one_at_the_limit_is_not`, `a_replayed_challenge_is_refused_the_proof_serves_once`, `a_forged_or_altered_challenge_is_refused`, `a_signature_by_another_key_or_over_other_bytes_is_refused`, `a_malformed_proof_never_panics_and_is_refused`, `the_challenge_reads_nothing_in_the_database_and_is_the_same_for_any_identifier`, `a_wrong_password_looks_the_same_with_or_without_a_proof_for_an_existing_or_missing_account`, `a_right_password_with_a_false_or_missing_proof_connects_as_if_there_were_no_key`.
- Revue r1 : `tests/device_proof.rs::{throwaway_keys_never_mute_the_proof_of_an_enrolled_device, a_small_order_public_key_is_never_verified_nor_enrolled, two_simultaneous_logins_with_the_same_challenge_and_the_right_password_one_wins, two_simultaneous_stream_proofs_with_the_same_challenge_one_serves, the_stored_public_key_is_compared_not_only_its_fingerprint}` ; domaine `weak_key::tests`, `challenge::tests::{an_owner_holds_at_most_its_share_and_only_blocks_itself, a_full_set_never_refuses_everybody_the_heaviest_owner_gives_up_its_oldest}` ; `infrastructure::crypto::tests::{ring_itself_accepts_the_neutral_point_key_so_the_domain_must_refuse_it, ring_refuses_a_non_canonical_scalar_s}`.
- `tests/device_http.rs` : `the_challenge_answers_the_same_for_an_existing_and_a_missing_identifier`, `the_challenge_is_public_versioned_and_strict_about_its_body`, `a_malformed_device_field_never_turns_a_login_into_an_error`, `a_wrong_password_answers_the_same_with_a_valid_proof_a_false_one_or_none`.
- `tests/device_stream.rs` : preuve sous l'empreinte du vrai certificat, de bout en bout (TLS 1.3, WebSocket).
- Client, contre un vrai agent (`crates/hearth-link/tests/device_key.rs`) : `every_stream_authentication_proves_the_key_again` (la dernière preuve de l'agent avance à chaque ouverture du flux : la signature liée au jeton est valide), `the_first_password_login_creates_the_key_and_the_agent_enrolls_this_pc` (défi `login` puis `session`), `an_old_agent_is_used_as_before_without_error_and_without_a_loop` (défi `404`, aucune clé, deux demandes en tout), `a_challenge_cut_at_login_connects_without_a_key`, `a_challenge_the_agent_never_issued_connects_and_keeps_no_key`, `bad_challenges_at_each_reconnection_keep_the_link_connected_and_announce_nothing_wrong` (défi coupé, forgé, rejoué, `404` : « Connecté », aucun état annoncé à tort), `removing_a_trusted_device_needs_the_password_and_the_proof_of_this_pcs_key` (usage `0x04`). `manager::device::tests` (hachage du jeton).

## Cas limites
- Un redémarrage de l'agent invalide les défis en cours : le client en redemande un.
- Un mot de passe faux ne consomme pas le défi (il peut servir ensuite avec le bon) ; un défi dont la preuve a servi ne resert pas.
- Agent d'avant HRT-22 : le défi répond `404` ; le client ne joint rien, ne crée pas de clé et n'insiste pas (au plus une sonde d'inscription par exécution).
- Lien avec le serveur, pas avec la session TLS elle-même (ADR-0023).

## Règles liées
- BR-TRUST-003, BR-TRUST-004, BR-TRUST-007, BR-CONN-013, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
