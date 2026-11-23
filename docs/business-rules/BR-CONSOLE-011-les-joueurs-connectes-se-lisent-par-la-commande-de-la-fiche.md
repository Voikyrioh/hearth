---
id: BR-CONSOLE-011
domaine: CONSOLE
titre: Les joueurs connectés se lisent par la commande de la fiche, jamais un chiffre deviné
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-011 : Les joueurs connectés

## Règle
Le nombre de joueurs, les places et les pseudos se lisent par la commande de la fiche (Minecraft : `list`), envoyée par l'agent lui-même, lue par un lecteur de réponse que la fiche nomme dans une liste fermée (`minecraft_list` : « There are N of a max of M players online: … » et la forme ancienne « There are N/M players online: … »). N et M vont de 0 à 100 000 ; 100 noms au plus sont gardés ; un nom hors de `\.?[A-Za-z0-9_]{1,16}` (le point des joueurs Bedrock est permis) est compté sans être rendu. Une réponse inconnue est « illisible » : jamais un chiffre deviné.

## Application (code)
- `crates/hearth-agent/src/domain/console/players.rs::{read, Reader}`
- Client (HRT-291) : `apps/desktop/src/services/players.ts::{playersShown, ageText}`, `composables/useOnlinePlayers.ts` (relue toutes les 30 s), `PlayersOnlineCard.vue`

## Vérification
- Client : `components/molecules/playersOnline.test.ts`
- `domain/console/players.rs::tests` (formes récente et ancienne, 0 joueur, nom Bedrock, nom hostile compté sans être rendu, nombres énormes refusés, réponse inconnue illisible, 100 noms au plus)

## Règles liées
- BR-CONSOLE-001, BR-CONSOLE-007

## Historique
- 2026-11-18 : création (HRT-267, le lecteur ; la route est HRT-268).
- 2026-11-23 : carte des joueurs côté client (HRT-291).
