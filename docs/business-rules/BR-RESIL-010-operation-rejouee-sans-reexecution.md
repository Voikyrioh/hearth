---
id: BR-RESIL-010
domaine: RESIL
titre: Rejouer une clé d'opération rend le premier résultat sans ré-exécuter
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-010), ADR-0004, HRT-04
maj: 2026-10-04
---

# BR-RESIL-010 — Opérations suivies par clé (côté agent)

## Règle
Toute requête qui modifie, faite par un compte connecté, peut porter `Idempotency-Key: <clé>`. L'agent enregistre la clé avant d'exécuter, puis le résultat (statut HTTP et corps) : rejouer la même clé rend le premier résultat (en-tête `Idempotent-Replayed: true`) sans ré-exécuter ; une clé dont l'exécution n'est pas finie répond `409 OPERATION_IN_PROGRESS` ; une clé déjà utilisée par un autre compte est refusée (`422 VALIDATION_ERROR`). Un résultat `5xx` n'est pas retenu (le client peut relancer). `GET /operations/{id}` relit l'état : `running`, `succeeded`, `failed`, ou `404` (jamais reçue : « non exécuté »). Les opérations sont conservées 24 h (`domain::operations::RETENTION`).

## Application (code)
- `crates/hearth-agent/src/domain/operations.rs::classify`, `OperationKey::parse`.
- `crates/hearth-agent/src/application/operations.rs::OperationService`.
- `crates/hearth-agent/src/entrypoint/http/operations.rs` — couche de clé et route `GET /operations/{id}`.

## Vérification
- Tests : `domain::operations::tests` ; `tests/sessions_https.rs::replaying_an_operation_key_returns_the_first_result_without_running_again`.

## Cas limites
- `POST /sessions` n'est pas suivi par clé : son résultat contient un jeton, qu'on ne conserve pas en base (les jetons n'y sont qu'en empreinte). Rejouer une connexion crée une nouvelle session.
- Une clé absente n'est pas une erreur : la requête s'exécute sans suivi.

## Règles liées
- BR-RESIL-009 (le client ne rejoue jamais seul une action incertaine).

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
