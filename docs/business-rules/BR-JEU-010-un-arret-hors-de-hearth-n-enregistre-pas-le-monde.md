---
id: BR-JEU-010
domaine: JEU
titre: Un arrêt qui ne passe pas par Hearth n'enregistre pas le monde : la carte « Arrêt du jeu » le dit en permanence
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-010), HRT-104
maj: 2026-11-14
---

# BR-JEU-010 : Un arrêt qui ne passe pas par Hearth n'enregistre pas le monde : la carte « Arrêt du jeu » le dit en permanence

## Règle
Hearth ne peut enregistrer le monde avant un arrêt que s'il le demande lui-même. Quand le serveur ou le moteur arrête le service sans passer par Hearth, aucun ordre n'est envoyé. Pour un jeu qui n'enregistre pas en s'arrêtant, le détail écrit en permanence, dans la carte « Arrêt du jeu » : « Si le serveur ou le moteur arrête ce jeu sans passer par Hearth, le monde n'est enregistré que par l'enregistrement automatique du jeu. ». Pour un jeu qui enregistre : « Ce jeu enregistre son monde en s'arrêtant. Il lui faut jusqu'à {délai}. ». La carte est visible de tous les profils.

## Application (code)
- `apps/desktop/src/services/game.ts::stopCardOf`
- `apps/desktop/src/components/molecules/GameStopCard.vue`

## Vérification
- `apps/desktop/src/services/game.test.ts`
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Aucune.

## Règles liées
- BR-JEU-001, BR-JEU-003, BR-JEU-008

## Historique
- 2026-11-14 : création (HRT-104, côté client).
