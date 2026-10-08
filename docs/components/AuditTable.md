# AuditTable

Organisme · `apps/desktop/src/components/organisms/AuditTable.vue`

Tableau du journal : grille ARIA (`grid`, en-têtes, `aria-rowcount`, `aria-busy`), lignes de hauteur fixe, **virtualisé** (seules les lignes visibles et quelques voisines sont dans le DOM). Ligne active au clavier (flèches, Début, Fin, Page précédente et suivante ; Entrée ouvre le détail ou déploie une rafale ; flèches droite et gauche sur une rafale). Rafales regroupées (`audit/grouping.ts`, décompte exact, adresse nommée seulement si certaine, rafale du bas non groupée tant qu'il reste à charger) en ligne teintée « X tentatives refusées en Y min » ; état déployé retenu par identifiants d'entrées. Valeurs en texte seulement (jamais en HTML), caractères de contrôle neutralisés, troncature avec le texte complet en infobulle. Date dans le fuseau du PC, source UTC dans l'infobulle.

- Props : `entries`, `busy`, `hasMore`, `loadingMore`, `scrollSignal`
- Événements et slots : `atTop`, `loadMore`, `open`
- Notes : Les dimensions sont posées en propriétés CSS (jamais `style=`). Tests : `pages/audit.test.ts`, `e2e/audit.spec.ts`.
- HRT-43 : la raison s'affiche avec une majuscule (`capitalize`). Alignement des en-têtes (C31) : non reproduit (garde `e2e/hrt43-44.spec.ts`, vert avant et après).
- Seconde passe : colonnes de 842 px minimum (tableau de 950 px, tient dans la carte à 1280), zone de lignes sans défilement horizontal, gouttière réservée aux deux bandes (en-têtes alignés). FIX:01M4EHYN77QBS7F7T5FCTRH6R5, FIX:01M4EHYNER6SJ3ZZVH286TD3FB.
