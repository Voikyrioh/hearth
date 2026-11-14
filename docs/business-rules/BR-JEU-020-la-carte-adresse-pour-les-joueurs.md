---
id: BR-JEU-020
domaine: JEU
titre: Le détail d'un serveur de jeu porte une carte « Adresse pour vos joueurs » : {adresse}:{port} et « Copier »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-020), HRT-104
maj: 2026-11-14
---

# BR-JEU-020 : Le détail d'un serveur de jeu porte une carte « Adresse pour vos joueurs » : {adresse}:{port} et « Copier »

## Règle
Le détail d'un service de jeu porte une carte « Adresse pour vos joueurs », visible de tous les profils, qui affiche l'adresse sous la forme « {adresse}:{port} » avec un bouton « Copier ». Pour un jeu à plusieurs ports, le port affiché est le port de base (le port de rôle « jeu », sinon le plus petit) et la carte ajoute « Ce jeu utilise d'autres ports : ils sont listés dans la carte « Ports à ouvrir ». ». Le succès de la copie dure 2 secondes sur le bouton (« Copiée ») et se dit aussi par la notification « Adresse copiée. » ; un échec dit de copier à la main. Lien coupé : l'adresse reste, « Copier » reste actif (copie locale).

## Application (code)
- `apps/desktop/src/components/molecules/GameAddressCard.vue`
- `apps/desktop/src/services/game.ts::addressOf`
- `apps/desktop/src/composables/useCopyText.ts`

## Vérification
- `apps/desktop/src/services/game.test.ts`
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Aucune.

## Règles liées
- BR-JEU-017, BR-JEU-021

## Historique
- 2026-11-14 : création (HRT-104, côté client).
