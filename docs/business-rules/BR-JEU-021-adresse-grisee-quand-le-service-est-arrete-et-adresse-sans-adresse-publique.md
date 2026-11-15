---
id: BR-JEU-021
domaine: JEU
titre: L'adresse est grisée quand le service est arrêté ; sans adresse publique, elle ne marche que depuis le réseau du serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-021), HRT-104
maj: 2026-11-14
---

# BR-JEU-021 : L'adresse est grisée quand le service est arrêté ; sans adresse publique, elle ne marche que depuis le réseau du serveur

## Règle
L'adresse affichée est celle sous laquelle le client joint ce serveur. Elle dit : « Cette adresse ne marche que depuis votre réseau. » (aucune adresse publique n'est connue de ce client). Service arrêté : le contenu de la carte est grisé et la carte ajoute « Le service est arrêté : personne ne peut se connecter. » (la phrase elle-même n'est pas grisée).

## Application (code)
- `apps/desktop/src/components/molecules/GameAddressCard.vue`
- `apps/desktop/src/components/organisms/GameCards.vue`

## Vérification
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Seule la moitié de la règle est livrée : l'adresse publique (champ facultatif, modifiable sans recréer le service, BR-JEU-022) n'a ni champ à l'installation, ni réglage, ni route d'agent. Suivi : ticket à créer, noté dans la session HRT-104.

## Règles liées
- BR-JEU-020, BR-JEU-022

## Historique
- 2026-11-14 : création (HRT-104, côté client).
