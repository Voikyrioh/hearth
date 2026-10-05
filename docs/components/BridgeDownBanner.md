# BridgeDownBanner

Molécule · `apps/desktop/src/components/molecules/BridgeDownBanner.vue`

Bandeau « Hearth n'arrive pas à lire la liste de tes serveurs. » avec « Réessayer », affiché par `App.vue` quand `servers.loadFailed` (le pont de liaison est en panne). `role="alert"`, jamais bloquant : la coquille reste utilisable.

- Props : `busy`
- Événements et slots : événement `retry`
- Notes : Suivi de la review HRT-09. Test : `router/connect.test.ts`.
