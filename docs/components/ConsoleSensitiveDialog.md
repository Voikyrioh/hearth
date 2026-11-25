# ConsoleSensitiveDialog

Organisme · `apps/desktop/src/components/organisms/ConsoleSensitiveDialog.vue`

La confirmation d'une commande de console (HRT-269, BR-CONSOLE-004, 005, maquette ConsoleSensible) : titre et phrase de la fiche, commande telle qu'elle partira, mot de passe demandé à chaque commande sensible, `AdminActDialog` autour (acte `console_command_sensitive` ; `console_command_free` quand l'état de confiance le demande).

- Props : serverId, pending, perform, refusalText
- Événements : close
- Notes : tests `pages/ServicesConsole.test.ts`, e2e `e2e/console.spec.ts`.
