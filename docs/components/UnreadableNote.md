# UnreadableNote

Molécule · `apps/desktop/src/components/molecules/UnreadableNote.vue`

Ligne discrète sous une liste dont l'agent a envoyé des éléments que cette version du client ne sait pas lire (BR-COMPAT-001 point 10) : « 1 élément n'est pas lisible par cette version de Hearth. Mets Hearth à jour sur ce PC. » ou « {n} éléments ne sont pas lisibles par cette version de Hearth. Mets Hearth à jour sur ce PC. ». N'affiche rien à zéro. Ne montre jamais le contenu d'un élément illisible, seulement leur nombre (`role="status"`, `data-unreadable`).

- Props : `count`
- Événements et slots : aucun
- Notes : posée sous les comptes (`Accounts.vue`), les postes de confiance (`Security.vue`), le journal d'activité (`Audit.vue`) et les services (`Services.vue`). Test : `pages/unreadableItems.test.ts`.
