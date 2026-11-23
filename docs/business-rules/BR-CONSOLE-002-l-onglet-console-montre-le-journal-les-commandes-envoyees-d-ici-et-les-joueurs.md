---
id: BR-CONSOLE-002
domaine: CONSOLE
titre: L'onglet Console montre le journal, les commandes envoyées d'ici avec leur réponse et les joueurs
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-23
---

# BR-CONSOLE-002 : L'onglet Console

## Règle
Un serveur dont l'agent parle à la console (capacité `game-console` et fiche qui l'ouvre à Hearth) a un onglet « Console » (ordre : Vue d'ensemble, Réglages, Mods, Console, Sauvegardes, Journaux). Il montre le journal du serveur en direct (visionneuse existante, « Mettre en pause »), les commandes envoyées DEPUIS CET ÉCRAN avec leur réponse (texte brut, jamais du HTML, codes de couleur retirés), un champ « Commande », « Envoyer », les commandes rapides de la fiche, les joueurs connectés et ce que Hearth surveille. Lire demande le droit de lire le service ; écrire demande le droit de gérer les services : sans lui, le champ et les commandes rapides sont inactifs (« Lecture seule. »). Les commandes et leurs réponses vivent dans l'écran : elles ne sont ni gardées par l'agent ni montrées sur un autre poste.

## Application (code)
- `apps/desktop/src/composables/useServiceConsole.ts`, `useServiceDoors.ts` (l'onglet existe quand `GET /services/{key}/console` dit `available`)
- `apps/desktop/src/components/organisms/ConsoleTab.vue`, `components/molecules/{ConsoleCommandField,ConsoleExchange}.vue`, `ConsoleSensitiveDialog.vue`
- `apps/desktop/src/services/console.ts`, `services/consoleLine.ts` (miroir de `hearth_proto::console`)

## Vérification
- `apps/desktop/src/pages/ServicesConsole.test.ts`, `services/consoleLine.test.ts`, `e2e/console.spec.ts`

## Règles liées
- BR-CONSOLE-003, 004, 005, 006, 007, 008, 011 ; BR-FICHE-051

## Historique
- 2026-11-23 : création (HRT-269).
