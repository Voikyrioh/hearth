# AuditFilters

Organisme · `apps/desktop/src/components/organisms/AuditFilters.vue`

Carte « Filtres » du journal : recherche, comptes, types d'action, résultats (listes à choix multiple), période (Tout l'historique, Aujourd'hui, 7 derniers jours, 30 derniers jours, Personnalisée avec « Du » et « Au »). Les filtres s'éditent ici et s'appliquent d'eux-mêmes (la page décide : recherche après la frappe, listes au choix, période personnalisée dès que ses deux dates sont valides ; aucun bouton « Appliquer ») ; « Effacer les filtres » apparaît dès qu'un filtre existe. Messages de période de la spec. Ne filtre rien : l'agent filtre.

- Props : `draft` (v-model `update:draft`), `accounts`, `active`, `busy`, `now`
- Événements et slots : `update:draft`, `apply`, `clear`
- Notes : Règles pures dans `audit/filters.ts`. Tests : `pages/audit.test.ts`.
- Carte sur une seule rangée aux 5 tailles : recherche d'au moins 200 px (`--audit-search-floor`), listes jamais coupées ; sous 820 px de carte « Effacer les filtres » s'affiche « Effacer » (nom accessible complet conservé).
