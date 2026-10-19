# ServiceRowMenu

Organisme · `apps/desktop/src/components/organisms/ServiceRowMenu.vue`

Le menu d'actions d'une ligne de la liste (HRT-65), pour les comptes qui ont le droit : bouton « Actions de {nom} » (`aria-haspopup="menu"`), entrées aux seuls gestes de l'état. Rendu dans `body` (la liste coupe ce qui la dépasse), placé à la main sous le bouton. Flèches, Début, Fin, Échap ; Tab le ferme. Bloqué (lien, moteur, verrou) : bouton `aria-disabled` avec la raison en infobulle, il n'ouvre rien. Rien de ce qui s'y passe ne remonte à la ligne (pas d'ouverture du détail).

- Props : `controls`, `name`
- Événements : `pick` (le geste choisi)
- Notes : monté par `ServiceTable` (colonne « Actions », prop `controlsFor`). Tests : `pages/ServicesLifecycle.test.ts`.
