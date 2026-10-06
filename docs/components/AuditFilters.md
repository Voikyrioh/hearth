# AuditFilters

Organisme · `apps/desktop/src/components/organisms/AuditFilters.vue`

Carte « Filtres » du journal : recherche, comptes, types d'action, résultats (listes à choix multiple), période (Tout l'historique, Aujourd'hui, 7 derniers jours, 30 derniers jours, Personnalisée avec « Du » et « Au »). Les filtres s'éditent ici et ne s'appliquent qu'au clic sur « Appliquer les filtres » (actif quand le brouillon diffère de l'appliqué, point d'attente) ou Entrée dans la recherche ; « Effacer les filtres » apparaît dès qu'un filtre existe. Messages de période de la spec. Ne filtre rien : l'agent filtre.

- Props : `draft` (v-model `update:draft`), `accounts`, `dirty`, `active`, `busy`, `now`
- Événements et slots : `update:draft`, `apply`, `clear`
- Notes : Règles pures dans `audit/filters.ts`. Tests : `pages/audit.test.ts`.
