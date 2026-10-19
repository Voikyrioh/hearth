---
id: BR-LIFE-008
domaine: LIFE
titre: Pour un service créé hors de Hearth, l'arrêt avertit que ses données peuvent être perdues, sans jamais refuser l'arrêt
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-008), HRT-65
maj: 2026-10-19
---

# BR-LIFE-008 — Pour un service créé hors de Hearth, l'arrêt avertit que ses données peuvent être perdues, sans jamais refuser l'arrêt

## Règle
La confirmation d'arrêt ou de redémarrage d'un service adopté ajoute, toujours visible et jamais repliée, que Hearth ne sait pas si ce programme enregistre ses données à l'arrêt et conseille de sauvegarder depuis le serveur en cas de doute. Deux informations s'y ajoutent selon le service : « Redémarre toujours » (il peut repartir au prochain redémarrage du serveur), ou service géré par le système du serveur (unité nommée, qui remplace la précédente : c'est le système qui le relance). Hearth ne refuse jamais l'arrêt d'un service : aucune liste de services protégés.

## Application (code)
- `apps/desktop/src/services/lifecycle.ts::warningsOf`
- `apps/desktop/src/components/organisms/ServiceLifecycleDialog.vue`

## Vérification
- `apps/desktop/src/services/lifecycle.test.ts (un service géré par le système remplace la règle « Redémarre toujours »)`
- `apps/desktop/src/pages/ServicesLifecycle.test.ts (un service créé hors de Hearth ajoute son avertissement)`

## Règles liées
- BR-LIFE-007, BR-ADOPT-012

## Historique
- 2026-10-19 — création (HRT-65 : démarrer, arrêter, redémarrer, côté client).
