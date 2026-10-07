---
id: BR-UPDATE-024
domaine: UPDATE
titre: Les refus de mise à jour sont consignés au journal d'activité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-024), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-024 : Les refus de mise à jour sont consignés au journal d'activité

## Règle
Sont consignés sous l'action « Mise à jour de l'agent » (`agent.update`) : le refus d'un compte en lecture seule (résultat « Refusé », raison « lecture seule »), la demande refusée parce qu'une mise à jour est en cours (« Échoué », « mise à jour déjà en cours »), l'installation gérée (« installation gérée par le système »), la signature refusée (« signature invalide »), et le résultat final de chaque mise à jour lancée : « Réussi », ou « Échoué » avec « téléchargement impossible », « signature invalide », « mise à jour échouée » ou « le nouvel agent n'a pas répondu, retour à la version précédente. Un changement du mode attaque fait depuis le début de la mise à jour a pu être annulé ». Cible : « version X.Y.Z ». Le résultat d'une mise à jour réussie ou revenue en arrière est écrit **une fois**, par l'agent qui revient (qui retrouve qui avait demandé dans le résultat).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/auth.rs::{guard, failure_of}` (lecture seule, installation gérée, signature).
- `crates/hearth-agent/src/application/update.rs::{UpdateService::start, fail, report}`, `audit_reason`.
- `crates/hearth-agent/src/domain/audit/{action.rs, event.rs}` (`AgentUpdate`, `Target::AgentVersion`, `Reason::*`).

## Vérification
- `tests/update_http.rs` (journal après chaque refus), `tests/update_use_cases.rs::{the_result_written_by_the_supervisor_is_announced_once_and_journaled_once_after_a_restart, a_rolled_back_update_is_announced_with_its_reason_and_journaled_as_failed}`.
- `deploy/e2e/scenario-update.sh` (journal relu après chaque scénario).

## Interface (HRT-17, lot interface)
- Le client ne pré-refuse PAS ce que l'agent refuse et consigne (rôle, installation gérée, déjà en cours) : la demande part et l'agent journalise son refus. Il ne refuse lui-même, sans rien envoyer (donc sans entrée au journal), que ce qu'il sait avant d'envoyer : aucune cible (`no_target`), version vue qui n'est plus celle retenue (`target_changed`), version qui n'est pas plus récente que l'agent (`not_newer`).
- Code : `apps/desktop/src-tauri/src/agent_update/service.rs::start`. Tests : `apps/desktop/src-tauri/tests/agent_update_runtime.rs` : `nothing_is_sent_without_a_target_for_another_version_or_for_a_downgrade`, `a_managed_installation_offers_no_update_and_the_agent_refuses_a_forced_request`.

## Cas limites
- Aucun secret : la cible est une version, la raison un texte fixe (BR-AUDIT-005).

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
