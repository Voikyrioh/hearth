---
id: BR-TRUST-028
domaine: TRUST
titre: Seul un administrateur, avec le mot de passe actuel et la preuve d'une clé inscrite de son compte, peut activer ou désactiver le mode attaque depuis le client
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-028) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-028 : Seul un administrateur, avec le mot de passe actuel et la preuve d'une clé inscrite de son compte, peut activer ou désactiver le mode attaque depuis le client

## Règle
`PUT /security/attack-mode {"active", "password", "device"}` (`Access::Admin`, suivie par clé d'opération). Ordre : (1) le rôle (`403 FORBIDDEN_ROLE` pour un compte Lecture seule, consigné par la couche d'accès, BR-TRUST-029) ; (2) la **preuve**, avant tout mot de passe : défi `purpose: "attack_mode"`, octet d'usage `0x03`, signature liée à l'empreinte du serveur, à l'identifiant, au **hachage du jeton** de la session appelante et à la **valeur demandée**, défi de 60 secondes à usage unique, clé **inscrite pour le compte appelant** : sinon `409 POST_NOT_RECOGNIZED` (ce code ne sert qu'à cette route) avec `details.reason` (`proof_missing` ou `proof_invalid`) et aucune écriture d'état (l'appelant est authentifié : le dire n'est pas un oracle) ; (3) le **mot de passe actuel** par le chemin de la connexion (mêmes compteurs, même ralentissement : un mot de passe faux est un échec de connexion, `422 WRONG_PASSWORD` ou `429`) ; la preuve a déjà fait des deux critères (clé + adresse), l'administrateur ne consomme pas d'essai unique ; (4) le changement et son entrée de journal, dans une transaction ; (5) le défi n'est consommé que si le changement a réussi (un mot de passe faux ne brûle pas la preuve).

Un administrateur sans clé inscrite (client ancien, poste non inscrit) ne peut ni activer ni désactiver depuis le client : commande sur le serveur (BR-TRUST-027) ou redémarrage physique. Le client le dit en clair (HRT-26).

Les actes de gestion des comptes, des sessions et la mise à jour de l'agent ne demandent pas encore mot de passe et clé (HRT-28, Q17).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/security.rs::set_attack_mode`.
- `crates/hearth-agent/src/application/sessions.rs::SessionService::set_attack_mode`, `crates/hearth-agent/src/application/trust.rs::TrustService::verify_attack_mode`.
- `hearth_proto::device_proof::{Binding::AttackMode, signing_bytes}`.

## Vérification
- `attack_mode_http.rs` : `a_read_only_account_is_refused_with_403_and_the_refusal_is_journaled`, `an_administrator_without_a_proved_key_is_refused_with_409_a_typed_reason_and_nothing_is_written`, `every_kind_of_wrong_proof_is_refused_with_409_and_leaves_the_mode_off` (clé non inscrite, clé d'un autre compte, autre usage, autre geste, autre jeton, périmée), `a_replayed_proof_is_refused_and_a_proof_that_did_not_serve_is_not_burned`, `a_wrong_password_counts_as_a_login_failure_with_the_same_counters_and_slowdown`, `an_administrator_with_a_proved_key_and_the_password_enables_then_disables_the_mode`, `the_route_needs_a_session_and_an_administrator`.
- `tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry` (balayage de `ENDPOINTS`).

## Cas limites
- Un poste volé (clé, session, mot de passe mémorisé au coffre) peut désactiver le mode s'il est administrateur : hors de portée de cette règle (le remède est de retirer le poste depuis un autre poste puis de changer le mot de passe).

## Règles liées
- BR-TRUST-010, 018, 027, 029, 030, ADR-0023, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
