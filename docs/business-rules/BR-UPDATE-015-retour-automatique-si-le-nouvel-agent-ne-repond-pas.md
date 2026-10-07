---
id: BR-UPDATE-015
domaine: UPDATE
titre: Retour automatique si le nouvel agent ne répond pas dans les 60 secondes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-015), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-015 : Retour automatique si le nouvel agent ne répond pas dans les 60 secondes

## Règle
Le superviseur (une copie de l'ancien binaire, détachée du service) arrête le service, échange les binaires (écriture atomique, l'ancien gardé de côté), redémarre, puis interroge `GET /hello` chaque seconde pendant 60 secondes. Le nouvel agent est tenu pour bon s'il répond avec la **nouvelle version** et le **même certificat** (BR-UPDATE-018). Sinon, ou si le certificat a changé, l'ancien binaire est remis **exactement tel qu'il était** (même octets), le service redémarre, et le résultat `rolled_back` est écrit (raison `no_answer` ou `identity_changed`). Si l'échange lui-même échoue, l'ancien binaire reste en place et le service est relancé (`failed`, raison `swap`). Si le retour en arrière échoue, le résultat dit `rollback_failed` et l'ancien binaire reste sauvegardé à côté du binaire installé (voir le runbook).

## Application (code)
- `crates/hearth-agent/src/domain/update/supervise.rs::check_verdict` (fonction pure : temps écoulé, dernière réponse, verdict).
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{run, drive, check}`.
- `crates/hearth-agent/src/application/install.rs` n'est pas concerné ; l'échange réutilise `InstallHost::{install_binary, restore_binary, discard_backup}` (BR-INSTALL-008).
- `crates/hearth-agent/src/infrastructure/update/host.rs::FsUpdateHost::launch` (`systemd-run`, unité transitoire hors du groupe de contrôle du service).

## Vérification
- `domain::update::supervise::tests`.
- `tests/update_supervisor.rs` (vrais fichiers) : `a_new_agent_that_never_answers_is_rolled_back_to_the_exact_old_binary`, `a_new_agent_with_another_certificate_is_rolled_back_at_once`, `an_agent_that_still_reports_the_old_version_is_not_a_success`, `a_swap_that_cannot_happen_leaves_the_old_binary_and_restarts_the_service`, `a_rollback_that_cannot_restart_the_old_agent_says_so_and_keeps_the_binary_restored`.
- `deploy/e2e/scenario-update.sh` (agent muet : retour automatique, binaire identique octet pour octet).

## Interface (HRT-17, lot interface)
- Retour arrière : « Mise à jour de l'agent annulée. Le nouvel agent n'a pas répondu. Retour à la version précédente. » (texte de la spécification), ton « annulé » ; la version affichée est celle d'avant et la mise à jour reste proposée. Raison `identity_changed` : texte propre sans inventer la raison pour les autres.
- Le résultat est annoncé UNE fois (notification discrète + carte), par la RELECTURE et non par l'étape `done` du flux (le nouvel agent l'annonce souvent avant le réabonnement) ; la coquille note sa date `at` (`ack_agent_result`) : pas de réannonce après un redémarrage du client.
- Code : `apps/desktop/src/agentUpdate/messages.ts::resultMessage`, `apps/desktop/src/stores/agentUpdates.ts::announceIfNew`, `apps/desktop/src-tauri/src/update/service.rs::{agent_result_seen, ack_agent_result}`.
- Tests : `apps/desktop/src/agentUpdate/messages.test.ts` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts` (« retour arrière »).

## Cas limites
- La fenêtre de 60 s part du redémarrage du service. Une version plus lente à démarrer est revenue en arrière : le runbook dit comment relancer à la main.
- Le service n'est jamais laissé arrêté sans que le résultat le dise.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
