# ServiceTable

Organisme · `apps/desktop/src/components/organisms/ServiceTable.vue`

Liste des services : grille ARIA virtualisée (hauteur de ligne déduite de la donnée, jamais mesurée), triée par la liaison, un seul arrêt de tabulation, ligne repère conservée à chaque mise à jour, noms et images coupés au milieu.

- Props : `services`. Mise en page : `services/layout.ts`. Tests : `components/organisms/serviceTable.test.ts`.
- HRT-63 : un clic, Entrée ou Espace sur une ligne émet `open` (clé stable) ; `selectedKey` marque la ligne ouverte (`aria-selected`) ; `focusKey(clé)` rend le focus à la ligne au retour. Sous 600 px (volet à côté du détail) : deux colonnes, Nom et État.
- HRT-65 : prop `controlsFor(service)` (absente : pas de colonne d'actions) ; la colonne « Actions » porte un `ServiceRowMenu` par ligne qui offre au moins un geste ; événement `gesture` (service, geste).
