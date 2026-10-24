# AppHeader

Organisme · `apps/desktop/src/components/organisms/AppHeader.vue`

En-tête de contenu : titre de la vue (Sora 30), emplacement d'actions, puis la pastille du lien du serveur, toujours visible (BR-RESIL-001).

- Props : `title`, `serverId` (sans lui, pas de pastille)
- Événements et slots : slot `actions`
- Notes : Dans `ServerLayout` le slot `actions` est `#header-actions` : les pages y téléportent leurs boutons avec `<Teleport defer to="#header-actions">`. Tests : `organisms.test.ts`.
- Le titre porte `data-page-title`. Une page peut le remplacer par le sien (`composables/usePageTitle.ts`, constat 17 de la revue UX du 2026-10-24) : « Terraria » pour la fiche, « Installer Terraria » à chaque étape de l'installation ; la page ne répète pas ce titre.
