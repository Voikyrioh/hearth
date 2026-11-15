---
id: BR-JEU-021
domaine: JEU
titre: L'adresse est grisée quand le service est arrêté ; sans adresse publique, elle ne marche que depuis le réseau du serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-021), HRT-104
maj: 2026-11-15
---

# BR-JEU-021 : L'adresse est grisée quand le service est arrêté ; sans adresse publique, elle ne marche que depuis le réseau du serveur

## Règle
L'adresse affichée est l'adresse publique que le propriétaire a renseignée, avec le port de base du jeu ; sans adresse publique, c'est celle sous laquelle le client joint ce serveur, et la carte dit : « Cette adresse ne marche que depuis votre réseau. Renseignez l'adresse publique de votre serveur pour afficher celle de vos joueurs. » Service arrêté : le contenu de la carte est grisé et la carte ajoute « Le service est arrêté : personne ne peut se connecter. » (la phrase elle-même n'est pas grisée).

## Application (code)
- `apps/desktop/src/components/molecules/GameAddressCard.vue`
- `apps/desktop/src/components/organisms/GameCards.vue`
- `apps/desktop/src/services/game.ts::addressOf`

## Vérification
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- L'adresse publique se renseigne après l'installation (BR-JEU-022) ; aucun champ dans le formulaire d'installation.

## Règles liées
- BR-JEU-020, BR-JEU-022

## Historique
- 2026-11-14 : création (HRT-104, côté client).
- 2026-11-15 : l'adresse publique est servie et modifiable (HRT-215).
