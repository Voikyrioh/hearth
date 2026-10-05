# AppHeader

Organisme · `apps/desktop/src/components/organisms/AppHeader.vue`

En-tête de contenu : titre de la vue (Sora 30), emplacement d'actions, puis la pastille du lien du serveur, toujours visible (BR-RESIL-001).

- Props : `title`, `serverId` (sans lui, pas de pastille)
- Événements et slots : slot `actions`
- Notes : Dans `ServerLayout` le slot `actions` est `#header-actions` : les pages y téléportent leurs boutons avec `<Teleport defer to="#header-actions">`. Tests : `organisms.test.ts`.
