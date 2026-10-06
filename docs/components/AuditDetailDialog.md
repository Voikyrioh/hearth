# AuditDetailDialog

Molécule · `apps/desktop/src/components/molecules/AuditDetailDialog.vue`

Une entrée du journal en entier (c'est là qu'on lit les valeurs tronquées du tableau), dans un `<dialog>` natif : piège à focus, Échap, retour du focus à la ligne. Texte non fiable : interpolation seulement, caractères de contrôle neutralisés, retour à la ligne des valeurs longues.

- Props : `entry` (`null` : fermée)
- Événements et slots : `close`
- Notes : Tests : `pages/audit.test.ts`.
