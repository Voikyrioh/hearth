---
id: BR-LIFE-022
domaine: LIFE
titre: Si le service change sous la fenêtre de confirmation, la fenêtre le dit et la confirmation devient inactive
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-022), HRT-65
maj: 2026-10-19
---

# BR-LIFE-022 — Si le service change sous la fenêtre de confirmation, la fenêtre le dit et la confirmation devient inactive

## Règle
Le service est suivi par sa clé stable dans la liste vivante pendant que la fenêtre est ouverte. S'il disparaît (« Ce service n'existe plus. »), si son état n'offre plus le geste (« Ce service vient de passer à l'état « {état} ». »), ou si une autre action le tient (« Une action est déjà en cours sur ce service. »), la fenêtre l'annonce (`role="status"`), le bouton de confirmation est `aria-disabled` et « Annuler » reste actif.

## Application (code)
- `apps/desktop/src/services/lifecycle.ts::staleNotice`
- `apps/desktop/src/components/organisms/ServiceLifecycleDialog.vue`

## Vérification
- `apps/desktop/src/pages/ServicesLifecycle.test.ts (l'état change sous la fenêtre)`

## Règles liées
- BR-LIFE-003, BR-LIFE-013

## Historique
- 2026-10-19 — création (HRT-65 : démarrer, arrêter, redémarrer, côté client).
