---
id: BR-JEU-004
domaine: JEU
titre: La confirmation d'arrêt ou de redémarrage d'un serveur de jeu dit ce que le jeu fait de son monde, à la place de la phrase générale
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-004), HRT-104
maj: 2026-11-14
---

# BR-JEU-004 : La confirmation d'arrêt ou de redémarrage d'un serveur de jeu dit ce que le jeu fait de son monde, à la place de la phrase générale

## Règle
La confirmation d'arrêt ou de redémarrage (BR-LIFE-007) est une seule fenêtre. Pour un serveur de jeu elle dit, selon ce que la fiche déclare : jeu qui n'enregistre pas en s'arrêtant, suite essayée : « Hearth va d'abord demander à {jeu} d'enregistrer son monde et attendre qu'il l'ait fait, puis arrêter le service (au plus {délai} pour l'arrêt). Tout ce qui est arrivé jusqu'à cet enregistrement sera conservé. » ; jeu qui enregistre en s'arrêtant : « {jeu} enregistre son monde en s'arrêtant. Il lui faut jusqu'à {délai} ; passé ce délai, le service est coupé de force et ce qu'il n'a pas eu le temps d'enregistrer peut être perdu. » ; jeu à qui Hearth ne peut rien demander (suite non essayée, jeu qui n'enregistre pas) : la phrase de BR-JEU-007. Ces phrases remplacent « Hearth ne peut pas encore envoyer d'ordre de sauvegarde au service » ; le délai est celui de la fiche, jamais une constante. Un service qui n'est pas un serveur de jeu (adopté, déployé à la main) garde la phrase générale.

## Application (code)
- `apps/desktop/src/services/game.ts::{stopWay, gameEffectText}`
- `apps/desktop/src/components/organisms/ServiceLifecycleDialog.vue`
- `apps/desktop/src/composables/useServiceLifecycle.ts::readGame`

## Vérification
- `apps/desktop/src/services/game.test.ts`
- `apps/desktop/src/pages/ServicesGame.test.ts`

## Cas limites
- Le « jusqu'à {durée} » de la spec (durée d'attente du signe) n'est pas dans la vue de l'agent : la phrase donne le délai d'arrêt de la fiche. Suivi dans la session HRT-104.
- La lecture de ce que l'agent dit du jeu échoue ou est absente : la phrase générale s'affiche, l'arrêt reste possible (BR-LIFE-008).

## Règles liées
- BR-JEU-002, BR-JEU-003, BR-JEU-007, BR-JEU-008, BR-LIFE-007

## Historique
- 2026-11-14 : création (HRT-104, côté client).
