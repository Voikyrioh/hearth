---
id: BR-CONN-020
domaine: CONN
titre: Les places des connexions en cours et des attentes du flux sont plafonnées, une part est réservée aux adresses connues
statut: active
invariant: true
source: contexts/hearth/tickets/hrt/HRT-20.md (critère 3 et suivi de la review de HRT-06), ADR-0022
maj: 2026-10-06
---

# BR-CONN-020 — Places réservées aux adresses connues

## Règle
> **Règle nouvelle**, absente de la spécification fonctionnelle : créée par le ticket HRT-20 (le suivi des places du flux est rattaché à ce ticket) et l'ADR-0022. **PROVISOIRE pour la notion d'adresse connue** : sera remplacée par l'identité d'appareil par clé (conception à venir).

**Connexions (`POST /sessions`)** : une connexion est traitée à la fois par adresse, huit au plus attendent leur tour (BR-CONN-007), et **32 au total** sont en cours (en attente ou en vérification). **8 de ces 32 places sont réservées** aux adresses déjà connues (d'un compte quelconque : une connexion réussie récente, ou une session valide) : une adresse inconnue n'en prend jamais plus de 24. La décision **ne dépend jamais de l'identifiant saisi** (sinon la réservation serait un oracle d'existence, BR-CONN-013). La lecture en base n'a lieu qu'en saturation, comme pour le flux. Au-delà : `429 TOO_MANY_ATTEMPTS`, `details.retry_after_s = 1`, tout de suite, mot de passe non gardé, aucun échec compté. Le plafond total (32) reste inférieur à la capacité du hacheur (4 calculs et 32 en attente) : une connexion admise ne reçoit jamais `503 BUSY` du hacheur.

**Flux temps réel (`GET /stream`)** : les connexions pas encore authentifiées sont au plus 2 par adresse et 16 au total (`docs/open-api/stream.md`, « Charge et plafonds »). **4 de ces 16 places sont réservées** aux adresses déjà connues : un inconnu n'en prend jamais plus de 12. Le quota par adresse (2) s'applique aussi aux adresses connues. Refus : `503 BUSY` (avec `Retry-After`). Une erreur de lecture vaut « inconnue ».

## Application (code)
- `crates/hearth-agent/src/domain/login_policy.rs::{admit_login, QueueRefusal, MAX_LOGINS_IN_FLIGHT, RESERVED_FOR_KNOWN}` ; `domain/lockout.rs::admits_in_queue`.
- `crates/hearth-agent/src/domain/stream.rs::{admit_pending, reserved_for_known, RESERVED_PENDING_FOR_KNOWN, Standing, PendingRefusal}`.
- `crates/hearth-agent/src/application/sessions.rs::{Turns::admit, SessionService::address_is_known}` ; `entrypoint/ws/mod.rs::{StreamContext::wait_for_place, stream}`.

## Vérification
- Domaine : `domain::login_policy::tests::{unknown_addresses_never_take_the_places_reserved_for_known_ones, the_per_address_queue_still_applies_to_known_addresses}`, `domain::stream::tests`, `infrastructure::argon2::tests::the_login_ceiling_stays_under_the_hasher_capacity`.
- Intégration : `tests/login_lockout.rs::unknown_addresses_never_take_the_places_reserved_for_known_ones` ; `tests/stream_https.rs::an_unknown_address_never_takes_the_waiting_places_reserved_for_known_ones` ; `tests/sessions_use_cases.rs::the_ninth_waiting_connection_of_an_address_is_refused_busy_at_once` ; `entrypoint::ws::tests`.

## Cas limites et limites connues
- Une place rendue par une adresse connue reste réservée. Les tables en mémoire ne dépassent jamais ces plafonds.
- Limites de la notion d'adresse connue (provisoire) : BR-CONN-019.

## Règles liées
- BR-CONN-007, BR-CONN-013, BR-CONN-019.

## Historique
- 2026-10-06 — création (HRT-20, ADR-0022). Ajustée après la review de la PR 23 : la réservation ne dépend plus de l'identifiant saisi.
