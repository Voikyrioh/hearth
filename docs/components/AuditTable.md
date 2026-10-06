# AuditTable

Organisme · `apps/desktop/src/components/organisms/AuditTable.vue`

Tableau du journal : grille ARIA (`grid`, en-têtes, `aria-rowcount`, `aria-busy`), lignes de hauteur fixe, **virtualisé** (seules les lignes visibles et quelques voisines sont dans le DOM). Ligne active au clavier (flèches, Début, Fin, Page précédente et suivante ; Entrée ouvre le détail ou déploie une rafale ; flèches droite et gauche sur une rafale). Rafales regroupées (`audit/grouping.ts`, décompte exact, adresse nommée seulement si certaine, rafale du bas non groupée tant qu'il reste à charger) en ligne teintée « X tentatives refusées en Y min » ; état déployé retenu par identifiants d'entrées. Valeurs en texte seulement (jamais en HTML), caractères de contrôle neutralisés, troncature avec le texte complet en infobulle. Date dans le fuseau du PC, source UTC dans l'infobulle.

- Props : `entries`, `busy`, `hasMore`, `loadingMore`, `scrollSignal`
- Événements et slots : `atTop`, `loadMore`, `open`
- Notes : Les dimensions sont posées en propriétés CSS (jamais `style=`). Tests : `pages/audit.test.ts`, `e2e/audit.spec.ts`.
