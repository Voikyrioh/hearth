---
id: BR-JEU-019
domaine: JEU
titre: Quand un port d'un serveur de jeu diffère de celui de la fiche, un rappel permanent dit de mettre à jour la box et le pare-feu
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-019), HRT-104
maj: 2026-11-14
---

# BR-JEU-019 : Quand un port d'un serveur de jeu diffère de celui de la fiche, un rappel permanent dit de mettre à jour la box et le pare-feu

## Règle
Quand un port d'un service de jeu est changé (installation avec un port différent de la fiche, ou modification ultérieure), la carte « Ports à ouvrir » montre les numéros réels et un rappel : « Les ports ont changé : mettez à jour votre box et votre pare-feu. ». Le rappel reste affiché tant que le service existe avec un port différent de celui de la fiche. L'agent marque chaque port dont le numéro diffère du numéro proposé par la fiche (`changed`).

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::game_view`
- `apps/desktop/src/services/game.ts::portsChanged`
- `apps/desktop/src/components/molecules/GamePortsCard.vue`

## Vérification
- `apps/desktop/src/services/game.test.ts`
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- La phrase supplémentaire pour un jeu qui annonce lui-même son port à l'extérieur (BR-FICHE-019) n'est pas écrite : aucune fiche livrée n'en a besoin.

## Règles liées
- BR-JEU-017, BR-FICHE-019

## Historique
- 2026-11-14 : création (HRT-104, côté client et agent).
