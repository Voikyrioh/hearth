---
id: BR-UPDATE-016
domaine: UPDATE
titre: Une mise à jour annulée n'est pas relancée automatiquement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-016), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-016 : Une mise à jour annulée n'est pas relancée automatiquement

## Règle
Une mise à jour annulée (retour en arrière) ou échouée est un résultat, pas une intention : rien n'est mémorisé pour la relancer. L'agent qui revient annonce le résultat une fois (journal et flux) puis n'en fait rien ; seule une nouvelle demande d'un administrateur lance une nouvelle mise à jour.

## Application (code)
- `crates/hearth-agent/src/application/update.rs::UpdateService::{resume, report_pending, report}` : lit, annonce, marque « annoncé » ; ne lance jamais.

## Vérification
- `tests/update_use_cases.rs::a_rolled_back_update_is_announced_with_its_reason_and_journaled_as_failed` (aucun superviseur lancé après).

## Interface (HRT-17, lot interface)
- Rien n'est relancé par le client après un retour arrière, une coupure ou un redémarrage : aucune mise à jour sans clic d'un administrateur (BR-UPDATE-002), et une action coupée avant sa réponse est « résultat inconnu », jamais rejouée (`LinkManager::execute`, `useServerAction`). La mise à jour annulée reste proposée et ne repart que sur un nouveau clic confirmé.
- Code : `apps/desktop/src/components/organisms/AgentUpdateCard.vue` (`confirm`), `apps/desktop/src/composables/useServerAction.ts`.
- Tests : `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts` (« does not replay an update cut before its answer… »).

## Cas limites
- Le client ne relance pas non plus à la reconnexion (`hearth-link`).

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
