---
id: BR-UPDATE-012
domaine: UPDATE
titre: Une seule mise à jour de l'agent à la fois
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-012), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-012 : Une seule mise à jour de l'agent à la fois

## Règle
Quand une mise à jour est déjà en cours, une nouvelle demande est refusée sans rien télécharger ni écrire : `409 OPERATION_IN_PROGRESS`, « Une mise à jour de l'agent est déjà en cours. Réessaye plus tard. ». « En cours » veut dire : une tâche de mise à jour vit dans ce processus, **ou** un superviseur tient le verrou de `update/` (le nouvel agent qui vient de redémarrer pendant le contrôle ne l'a pas lancé mais le voit). Le verrou est un verrou de fichier : le système le relâche si le superviseur meurt. Le refus est consigné (BR-UPDATE-024).

## Application (code)
- `crates/hearth-agent/src/domain/update/target.rs::plan_update` : l'ordre des refus (installation gérée, déjà en cours, puis cible).
- `crates/hearth-agent/src/application/update.rs::UpdateService::start` : décision et prise de la place sous un seul verrou (deux demandes simultanées : une seule passe).
- `crates/hearth-agent/src/infrastructure/update/host.rs::FsUpdateHost::{supervisor_running, take_supervisor_lock}`.

## Vérification
- `domain::update::target::tests` (message exact, ordre des refus).
- `tests/update_use_cases.rs` : `only_one_update_at_a_time_and_the_refusal_is_journaled`, `a_supervisor_that_still_works_counts_as_an_update_in_progress_after_a_restart`.
- `tests/update_http.rs` : `a_second_request_during_an_update_is_refused_with_the_spec_message_and_journaled_once`.
- `infrastructure::update::host::tests::the_supervisor_lock_is_exclusive_and_released_on_drop`.
- `deploy/e2e/scenario-update.sh` (deuxième demande pendant un téléchargement lent).

## Interface (HRT-17, lot interface)
- La demande « déjà en cours » est refusée par l'AGENT (`409 OPERATION_IN_PROGRESS`, pas de pré-refus local, pour que le refus soit consigné, BR-UPDATE-024) : l'interface la montre « Une mise à jour de l'agent est déjà en cours. Réessaye plus tard. » (texte de la spécification fonctionnelle ; le ticket et l'agent disent « Une mise à jour est déjà en cours… », écart tranché en faveur de la spécification) et relit l'état, qui montre l'avancement de l'autre. Pendant une mise à jour le bouton est désactivé.
- Code : `apps/desktop/src-tauri/src/agent_update/wire.rs::refusal_from_error` (`InProgress`), `apps/desktop/src/components/organisms/AgentUpdateCard.vue`.
- Tests : `apps/desktop/src-tauri/tests/agent_update_runtime.rs` : `a_second_request_while_one_runs_is_refused_by_the_agent` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts` (« says an update is already running… »).

## Cas limites
- Une clé d'opération rejouée (coupure du client) renvoie la première réponse `202` sans relancer (BR-UPDATE-017).

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
