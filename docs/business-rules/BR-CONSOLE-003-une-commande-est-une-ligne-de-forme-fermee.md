---
id: BR-CONSOLE-003
domaine: CONSOLE
titre: Une commande de console est une seule ligne de forme fermée ; sa classe se juge sur son premier mot selon la fiche
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-003 et 004 : La forme et la classe d'une commande

## Règle
Une commande est une seule ligne de 1 à 256 points de code, sans caractère de contrôle (`\n`, `\r`, `\t`, `\0` compris) ni caractère bidirectionnel ou invisible. Les espaces de tête et de fin et UNE barre oblique de tête sont retirés ; le texte envoyé est ensuite celui de la ligne, sans autre changement. Sa classe se juge sur son premier mot (minuscules ASCII, préfixe `minecraft:` retiré), selon la liste que la FICHE déclare : refusée (jamais envoyée, avec une raison de liste fermée), sensible (mot de passe exigé) ou libre. La fonction est pure et partagée : le client classe pour l'écran, l'agent reclasse pour décider.

## Application (code)
- `crates/hearth-proto/src/console.rs::{normalize, classify, ConsolePolicy}`

## Vérification
- `hearth-proto/src/console.rs::tests` (forme, 256 points de code acceptés et 257 refusés, caractères interdits, `/minecraft:op Bram` sensible, `STOP` refusée, seul le premier mot juge)
- `hearth-proto/src/console.rs::tests` : un verbe de mod (`coolmod:wipe`) est sensible, `minecraft:list` libre
- `apps/desktop/src/services/consoleLine.test.ts` : la même table de cas côté client (`classifyLine`), titre et phrase génériques i18n (`console.confirm.modTitle`, `modSentence`) pour un verbe à espace de noms ; même règle dans `link/simulated-console.ts` (qui passe par `classifyLine`)

## Règles liées
- BR-CONSOLE-001, BR-CONSOLE-005

## Historique
- 2026-11-18 : création (HRT-267).
- 2026-11-23 : un verbe à espace de noms inconnu (`mod:commande`) est sensible par défaut ; `function`, `schedule`, `datapack` sensibles dans la fiche Minecraft (revue Stephen, ADR-0067 points 11 et 12).
- 2026-11-23 : le miroir client (`classifyLine`) applique la même règle du verbe à espace de noms (seconde passe Stephen, I1).
