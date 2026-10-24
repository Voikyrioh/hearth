# Services

Page · `apps/desktop/src/pages/Services.vue`

Section « Services » d'un serveur (HRT-62), en LECTURE SEULE : la carte du moteur d'abord (`EnginePanel`), puis la liste (`ServiceTable`), l'état vide, les lignes esquissées de la première détection, l'erreur de lecture, la mention « Liste du moteur d'il y a… » et le message d'agent trop ancien. Alimentée par `stores/services.ts` ; deux rôles, même vue.

- Aucune prop. Route `/servers/:id/services`. Tests : `pages/Services.test.ts`, `e2e/services.spec.ts`.
- HRT-69 : la barre « Déployer un service » (`DeployButton`) au-dessus de la liste, ou dans l'état vide.
