---
id: BR-JEU-018
domaine: JEU
titre: Hearth n'ouvre jamais de port sur la box ni sur le pare-feu et ne vérifie pas qu'un port est joignable depuis Internet
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-018), HRT-104
maj: 2026-11-14
---

# BR-JEU-018 : Hearth n'ouvre jamais de port sur la box ni sur le pare-feu et ne vérifie pas qu'un port est joignable depuis Internet

## Règle
Hearth n'ouvre jamais de port sur la box, ni sur le pare-feu du serveur, ni par un protocole d'ouverture automatique. Il ne teste pas non plus qu'un port est joignable depuis Internet ; le détail le dit : « Hearth ne peut pas vérifier que ces ports sont joignables depuis Internet. ». La carte d'adresse dit de même « Hearth ne peut pas vérifier que cette adresse est joignable. ». Aucune carte ne promet une joignabilité.

## Application (code)
- `apps/desktop/src/components/molecules/GamePortsCard.vue`
- `apps/desktop/src/components/molecules/GameAddressCard.vue`

## Vérification
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Aucune.

## Règles liées
- BR-JEU-017, BR-JEU-021

## Historique
- 2026-11-14 : création (HRT-104, côté client).
