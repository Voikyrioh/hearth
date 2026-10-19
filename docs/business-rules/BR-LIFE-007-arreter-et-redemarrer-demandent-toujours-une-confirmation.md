---
id: BR-LIFE-007
domaine: LIFE
titre: Arrêter et redémarrer demandent toujours une confirmation de l'effet, avec le délai d'arrêt réel du service
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-007), HRT-65
maj: 2026-10-19
---

# BR-LIFE-007 — Arrêter et redémarrer demandent toujours une confirmation de l'effet, avec le délai d'arrêt réel du service

## Règle
Arrêter et redémarrer ouvrent TOUJOURS la fenêtre de confirmation, quel que soit l'état, l'origine ou les ports du service : elle dit que le service reçoit l'ordre de s'arrêter proprement, qu'il est coupé de force s'il ne s'est pas arrêté au bout de SON délai d'arrêt (lu par l'agent, jamais une constante de l'interface ; sans valeur connue, la phrase le dit sans nombre) et que Hearth ne peut pas encore envoyer d'ordre de sauvegarde au service. La confirmation de l'effet et celle du socle sont une seule fenêtre (`AdminActDialog`). La phrase d'une fiche de serveur de jeu (BR-JEU-004) viendra de HRT-104 ; démarrer n'a pas d'effet à confirmer.

## Application (code)
- `apps/desktop/src/services/lifecycle.ts::{effectText, graceSeconds}`
- `apps/desktop/src/components/organisms/ServiceLifecycleDialog.vue`
- `apps/desktop/src/composables/useServiceLifecycle.ts::request`

## Vérification
- `apps/desktop/src/pages/ServicesLifecycle.test.ts (arrêter : « demande une confirmation qui dit l'effet et le délai réel »)`
- `apps/desktop/e2e/hrt65-lifecycle.spec.ts`

## Règles liées
- BR-LIFE-008, BR-LIFE-009, BR-LIFE-016, BR-LIFE-017

## Historique
- 2026-10-19 — création (HRT-65 : démarrer, arrêter, redémarrer, côté client).
