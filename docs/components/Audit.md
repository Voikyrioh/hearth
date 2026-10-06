# Audit

Page · `apps/desktop/src/pages/Audit.vue`

Journal d'activité (HRT-14), réservé aux administrateurs : carte de filtres (`AuditFilters`), compteur, tableau virtualisé (`AuditTable`), bouton flottant « N nouvelles entrées », message de conservation, boîte de détail (`AuditDetailDialog`). « Exporter » dans l'en-tête (`needs-link`, désactivé sans entrée). Hors « Connecté » : étiquette « Périmé », « Données périmées, serveur injoignable » et « Rechargement manuel » (la désaturation et la date viennent du gabarit, la page ne s'enveloppe pas).

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/audit` (`meta.adminOnly`). Ouvre le journal du serveur affiché au montage et le referme au démontage (`stores/audit.ts`). Rien n'est mémorisé d'une ouverture à l'autre. Tests : `pages/audit.test.ts`, `stores/audit.test.ts`, `e2e/audit.spec.ts`. Règles : BR-AUDIT-001 à 021 (sections « Interface »).
