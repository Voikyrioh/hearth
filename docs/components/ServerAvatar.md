# ServerAvatar

Molécule · `apps/desktop/src/components/molecules/ServerAvatar.vue`

Avatar rond d'un serveur : initiales, anneau de la couleur du serveur quand il est actif, pastille d'état du lien en bas à droite. Le nom accessible est « {nom}, {état} » : l'état ne dépend pas de la couleur.

- Props : `name`, `color` (1 à 8 : jetons `--server-1` à `--server-8`), `state`, `active`, `mark` (HRT-26 : `attack`, `suspended` ou `alert`, ou rien)
- Événements et slots : aucun
- Notes : La couleur est un numéro de palette (pas de `style=` en ligne). Tests : `molecules.test.ts`.
- HRT-26 : `mark` pose un petit rond en haut à droite (bouclier rose : mode attaque actif ; horloge turquoise : suspendu ; triangle ambre : alerte), jamais la couleur seule : le nom accessible devient « {nom}, {état}, mode attaque actif » (ou « suspendu », « alerte de sécurité »). Priorité : mode attaque, suspendu, alerte (`security/mark.ts::markOf`). Tests : `pages/SecurityMode.test.ts`.
