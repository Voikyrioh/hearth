---
id: BR-DEPLOY-009
domaine: DEPLOY
titre: Quand un port est refusé, l'agent propose le plus petit port libre supérieur, à défaut le plus grand libre inférieur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-009), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-009 — Quand un port est refusé, l'agent propose le plus petit port libre supérieur, à défaut le plus grand libre inférieur

## Règle
Libre : ni dans le formulaire, ni tenu ou réservé par un conteneur (géré ou non), ni écouté par un programme du serveur. La proposition essaie au plus 4 096 numéros de chaque côté.

## Application (code)
- `crates/hearth-agent/src/domain/services/ports.rs::suggest`
- `apps/desktop/src/components/molecules/PortMappingRow.vue (data-port-use)` et `composables/useDeployForm.ts::useSuggested` (client : le port proposé par l'agent, jamais recalculé)

## Vérification
- `crates/hearth-agent/src/domain/services/ports.rs::tests::the_suggestion_is_the_smallest_free_port_above_then_the_largest_below`
- `crates/hearth-agent/src/domain/services/ports.rs::tests::a_port_reserved_by_a_stopped_service_is_not_suggested`
- `apps/desktop/src/pages/DeployService.test.ts (un port tenu par un service bloque…)`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-008

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
