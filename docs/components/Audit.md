# Audit

Page · `apps/desktop/src/pages/Audit.vue`

Journal d'activité (HRT-14), réservé aux administrateurs : carte de filtres (`AuditFilters`), compteur, tableau virtualisé (`AuditTable`), bouton flottant « N nouvelles entrées », message de conservation, boîte de détail (`AuditDetailDialog`). « Exporter » dans l'en-tête (`needs-link`, désactivé sans entrée). Hors « Connecté » : étiquette « Périmé », « Données périmées, serveur injoignable » et « Rechargement manuel » (la désaturation et la date viennent du gabarit, la page ne s'enveloppe pas).

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/audit` (`meta.adminOnly`). Ouvre le journal du serveur affiché au montage et le referme au démontage (`stores/audit.ts`). Rien n'est mémorisé d'une ouverture à l'autre. Tests : `pages/audit.test.ts`, `stores/audit.test.ts`, `e2e/audit.spec.ts`. Règles : BR-AUDIT-001 à 021 (sections « Interface »).
- HRT-38 (C46) : journal jamais lu hors « Connecté » : « Pas encore chargé… » (par état du lien), pas d'estampille (`usePageData`).
- HRT-38 (C30) : plus de bloc « Périmé / Données périmées / Rechargement manuel » ni de message d'échec hors « Connecté » : le bandeau et l'estampille du gabarit suffisent.
- HRT-43 : la recherche s'applique seule 350 ms après la frappe ; les listes s'appliquent au choix, une période personnalisée dès que ses deux dates sont valides, sans bouton « Appliquer » ; l'état « Aucun événement ne correspond » n'a plus son propre « Effacer les filtres ». FIX:01M4DNJ42ETVYR5Y01YNVPW715. Test : `e2e/hrt43-44.spec.ts`.
- HRT-43 : après un échec de lecture, « Ta recherche n'est pas appliquée. … » et « Réessayer » relance la recherche tapée. FIX:01M4ECZK55R0HTKS7SQH4WZ1ZZ.
