---
id: BR-SVC-031
domaine: SVC
titre: Pendant qu'une action tient un service, les boutons de pilotage sont désactivés
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-031), HRT-84
maj: 2026-10-17
---

# BR-SVC-031 — Pilotage désactivé pendant une action

## Règle
Pendant qu'une action tient le verrou d'un service (BR-LIFE-013), y compris une sauvegarde ou une restauration, les boutons de pilotage du service sont désactivés avec « Une action est déjà en cours sur ce service. » et « Lancée par {compte}. », quel que soit l'état réel affiché. L'indication de qui vient de `ServiceItem.busy` (absente pour un compte qui ne lit pas les opérations du service) ; un identifiant illisible n'est jamais nommé. Aucun bouton de pilotage n'existe encore : `pilotLock` est le verdict que ceux des tickets suivants liront.

## Application (code)
- `apps/desktop/src/services/pilot.ts::pilotLock`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (phrase dans le détail)
- `apps/desktop/src-tauri/src/services/dto.rs::BusyDto`

## Vérification
- `apps/desktop/src/services/transition.test.ts`
- `apps/desktop/src/pages/ServicesOperations.test.ts`

## Historique
- 2026-10-17 — création (HRT-84).
