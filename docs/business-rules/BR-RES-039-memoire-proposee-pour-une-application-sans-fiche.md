---
id: BR-RES-039
domaine: RES
titre: Application sans fiche : 2 Go, ou la moitié de la mémoire libre si elle est plus petite, jamais moins de 256 Mo ; tous les processeurs
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-038, BR-RES-039), HRT-146
maj: 2026-10-17
---

# BR-RES-039 — Application sans fiche : 2 Go, ou la moitié de la mémoire libre si elle est plus petite, jamais moins de 256 Mo ; tous les processeurs

## Règle
Pour une application sans fiche, Hearth n'a aucun chiffre. La mémoire proposée est 2 Go, ou la moitié de la mémoire encore disponible si cette moitié est plus petite, jamais moins de 256 Mo et jamais plus que la machine. « Disponible » : mémoire totale, moins la réserve, moins ce qui est déjà engagé par les services en marche. Le processeur proposé est tous les processeurs du serveur. Sous le curseur de mémoire : « Hearth n'a pas de chiffre pour ce programme. » puis « Hearth propose {x} parce qu'il ne connaît pas cette application. Montez ce chiffre si elle en demande plus. ». Pour une fiche, le curseur part de la mémoire recommandée (`BR-RES-038`), ramenée au maximum de la machine si elle le dépasse, avec sa preuve (« mesuré » ou « chiffre de l'éditeur ») ; sous cette valeur, « Sous la mémoire recommandée : le serveur peut ralentir ou s'arrêter. » s'affiche sans bloquer. L'utilisateur change librement les deux valeurs.

## Application (code)
- `apps/desktop/src/resources/start.ts::{proposedMemoryBytes, proposedCpuMillis, startingResources}`
- `apps/desktop/src/components/organisms/ResourceSliders.vue` (phrases sous le curseur de mémoire)

## Vérification
- `apps/desktop/src/resources/resources.test.ts` (valeurs de départ)
- `apps/desktop/src/components/organisms/resourceSliders.test.ts` (phrases, avertissement non bloquant)

## Cas limites
- Machine déjà pleine : 256 Mo, jamais zéro.
- Recommandation de fiche au-dessus de la machine : curseur ramené au maximum, l'avertissement « Sous la mémoire recommandée » reste vrai.

## Règles liées
- BR-RES-004, BR-RES-037

## Historique
- 2026-10-17 — HRT-146 : côté client.
