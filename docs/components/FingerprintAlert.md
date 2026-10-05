# FingerprintAlert

Organisme · `apps/desktop/src/components/organisms/FingerprintAlert.vue`

Alerte d'identité changée (BR-CONN-003) : seul écran bloquant de l'application. `<dialog>` natif modal, deux empreintes côte à côte (« Empreinte mémorisée », « Empreinte reçue » bordée de `--crit`), « Ne pas se connecter » par défaut (focus à l'ouverture, Échap) et « Accepter la nouvelle empreinte » en bouton contour, jamais plein.

- Props : `change` (empreintes), `serverName`
- Événements et slots : événements `refuse`, `accept`
- Notes : Montée par `App.vue` pour le serveur affiché (sinon le premier en attente). Tests : `connect.test.ts`, `router/connect.test.ts`, `e2e/connect.spec.ts`.
