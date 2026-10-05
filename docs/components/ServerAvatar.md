# ServerAvatar

Molécule · `apps/desktop/src/components/molecules/ServerAvatar.vue`

Avatar rond d'un serveur : initiales, anneau de la couleur du serveur quand il est actif, pastille d'état du lien en bas à droite. Le nom accessible est « {nom}, {état} » : l'état ne dépend pas de la couleur.

- Props : `name`, `color` (1 à 8 : jetons `--server-1` à `--server-8`), `state`, `active`
- Événements et slots : aucun
- Notes : La couleur est un numéro de palette (pas de `style=` en ligne). Tests : `molecules.test.ts`.
