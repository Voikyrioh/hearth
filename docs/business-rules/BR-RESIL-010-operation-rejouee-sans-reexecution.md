---
id: BR-RESIL-010
domaine: RESIL
titre: Rejouer une clé d'opération rend le premier résultat sans ré-exécuter
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-010), ADR-0004, HRT-04
maj: 2026-10-05
---

# BR-RESIL-010 — Opérations suivies par clé (côté agent)

## Règle
Toute requête authentifiée qui modifie (routes marquées « suivie » dans `ENDPOINTS`) peut porter `Idempotency-Key: <clé>`. La clé est celle d'un compte (clé primaire `(account_id, id)` : deux comptes peuvent choisir la même sans se voir) et liée à la requête : méthode, chemin et SHA-256 du corps. L'agent enregistre la clé avant d'exécuter, puis le résultat (statut HTTP et corps) :
- même clé, même requête, terminée : le premier résultat est rendu (en-tête `Idempotent-Replayed: true`) sans ré-exécuter ;
- exécution pas finie : `409 OPERATION_IN_PROGRESS` ;
- même clé, autre requête : `422 IDEMPOTENCY_KEY_REUSED`, sans exécuter ;
- exécution interrompue par un arrêt de l'agent (état `interrupted`, posé au démarrage) : `409 CONFLICT`, résultat inconnu, jamais rejoué.

La requête suivie s'exécute dans une tâche détachée, dans le span de la requête (un handler qui panique laisse la clé oubliée, jamais « en cours ») : un client qui coupe avant la réponse retrouve le résultat (BR-RESIL-010 : « Fait pendant la coupure »). Un résultat `5xx` n'est pas retenu (le client peut relancer). `GET /operations/{id}` relit l'état : `running`, `succeeded`, `failed`, `interrupted`, ou `404` (jamais reçue : « non exécuté »). Les opérations sont conservées 24 h (`domain::operations::RETENTION`).

## Application (code)
- `crates/hearth-agent/src/domain/operations.rs::{classify, RequestFingerprint::of, OperationKey::parse}`.
- `crates/hearth-agent/src/application/operations.rs::OperationService`.
- `crates/hearth-agent/src/entrypoint/http/operations.rs::{track, get}` — suivi (posé par `auth::guard` pour les routes `tracked`) et route `GET /operations/{id}` ; `application/operations.rs::OperationService::interrupt_running` — appelé par `app::start_with`.
- Côté client (HRT-07) : `crates/hearth-link/src/domain/pending_ops.rs::PendingOps::{register, link_lost, to_resolve, resolve}` — la clé voyage dans `Idempotency-Key`, l'action coupée est « résultat inconnu » et n'est jamais rejouée ; au retour du lien, `GET /operations/{id}` donne l'une des trois issues (`Outcome` : fait pendant la coupure, non exécuté, résultat inconnu). `succeeded` donne « fait pendant la coupure » ; `404` et `failed` donnent « non exécuté » ; `interrupted`, `running` après 5 relectures et toute opération de plus de 24 h donnent « résultat inconnu ». Si la session suivante est celle d'un **autre compte**, ou après acceptation d'une nouvelle empreinte, les clés d'opération ne disent plus rien : toutes sont soldées en « résultat inconnu » sans interroger l'agent (jamais « non exécuté » par erreur ; `PendingOps::settle_all`).

## Vérification
- Tests : `domain::operations::tests` ; `tests/sessions_use_cases.rs` (clé liée à la requête, par compte, interruption) ; `tests/http_api.rs::a_key_reused_for_another_request_is_refused_without_running` ; `tests/sessions_https.rs::replaying_an_operation_key_returns_the_first_result_without_running_again`, `::a_client_that_cuts_before_the_answer_still_gets_its_result_recorded`, `::an_operation_left_running_by_a_previous_run_is_interrupted_at_startup`.
- Tests côté client : `domain::pending_ops::tests` (une issue par réponse de l'agent) ; `crates/hearth-link/tests/fault_proxy.rs::an_action_cut_before_the_answer_is_unknown_and_never_replayed` (fait pendant la coupure), `::an_action_that_never_reached_the_agent_is_announced_as_not_executed`, `::an_action_interrupted_by_the_agent_stopping_stays_unknown`.
- Coquille : `apps/desktop/src-tauri/tests/offline.rs``::an_action_cut_before_the_answer_is_unknown_never_replayed_and_its_outcome_comes_back` (une seule issue `link://operation`, « done »).
- Interface : `apps/desktop/src/stores/offline.test.ts``::annonce les trois issues avec les textes de la spec`.

## Cas limites
- `POST /sessions` n'est pas suivi par clé : son résultat contient un jeton, qu'on ne conserve pas en base (les jetons n'y sont qu'en empreinte). Rejouer une connexion crée une nouvelle session.
- Une clé absente n'est pas une erreur : la requête s'exécute sans suivi.

## Affichage dans le client
- L'interface annonce l'issue par une notification discrète : `apps/desktop/src/stores/link.ts` (`OPERATION_TEXTS`), voir BR-RESIL-011.
- Les trois issues s'affichent en notification discrète nommant le serveur : « Fait pendant la coupure. », « Non exécuté. Tu peux relancer. », « Résultat inconnu. Vérifie l'état du serveur. » (`stores/link.ts::onOperation`, textes `operation.*`).

## Règles liées
- BR-RESIL-009 (côté client : jamais de rejeu automatique).
- BR-RESIL-009 (le client ne rejoue jamais seul une action incertaine).

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
- 2026-10-05 — côté client ajouté (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- 2026-10-05 : vérifications de la coquille et de l'interface (HRT-12).
