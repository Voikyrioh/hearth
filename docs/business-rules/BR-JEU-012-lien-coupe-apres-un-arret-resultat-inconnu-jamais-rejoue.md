---
id: BR-JEU-012
domaine: JEU
titre: Si le lien coupe après l'envoi d'un arrêt ou d'un redémarrage de jeu, le résultat n'est pas connu et rien n'est rejoué
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-012), HRT-104
maj: 2026-11-14
---

# BR-JEU-012 : Si le lien coupe après l'envoi d'un arrêt ou d'un redémarrage de jeu, le résultat n'est pas connu et rien n'est rejoué

## Règle
Si le lien coupe après l'envoi d'un arrêt ou d'un redémarrage, Hearth affiche « Le résultat de cette action n'est pas connu. » et ne le rejoue jamais (BR-RESIL-009). Le serveur va au bout de la séquence ; au retour du lien, Hearth relit l'état réel du service et annonce l'issue (BR-RESIL-010).

## Application (code)
- `apps/desktop/src/composables/useServiceLifecycle.ts::followGame`
- `apps/desktop/src/components/organisms/GameSequenceDialog.vue`

## Vérification
- `apps/desktop/src/pages/ServicesGame.test.ts (résultat inconnu)`

## Cas limites
- Aucune.

## Règles liées
- BR-LIFE-020, BR-RESIL-009, BR-RESIL-010

## Historique
- 2026-11-14 : création (HRT-104, côté client).
