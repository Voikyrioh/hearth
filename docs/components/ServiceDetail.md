# ServiceDetail

Organisme · `apps/desktop/src/components/organisms/ServiceDetail.vue`

Détail d'un service (HRT-63, BR-SVC-007 à 009, 016, 027, 028), à côté de la liste (route `services?service={clé stable}`). Données de la liste déjà chargée ; service disparu : lisible, grisé, homonyme proposé jamais ouvert d'office. Démarrer, arrêter, redémarrer et l'onglet Journaux ne sont pas branchés : une phrase le dit.

- Props : `serverId`, `opened` (`services/detail.ts::openedService`), `gate` (droit), `reasonId`, `connected`, `engineReady`, `focusTitle`. Émet `back`, `openOther`, `retryRights`.
- Tests : `pages/ServicesDetail.test.ts`, `services/detail.test.ts`, `e2e/hrt63-detail.spec.ts`.
