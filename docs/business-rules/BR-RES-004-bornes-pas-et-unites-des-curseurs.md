---
id: BR-RES-004
domaine: RES
titre: Chaque curseur est borné par la machine, avance par crans permis et se double d'une saisie exacte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-curseurs-ressources.md (BR-RES-004), contexts/hearth/conceptions/2026-10-13-design-deploiements-lots-suivants.md (4.2), HRT-146
maj: 2026-10-17
---

# BR-RES-004 — Chaque curseur est borné par la machine, avance par crans permis et se double d'une saisie exacte

## Règle
Le maximum du curseur de processeur est le nombre de processeurs logiques du serveur, celui de la mémoire sa mémoire totale, lus sur la machine (`GET /engine/capacity`) et non dans un conteneur. Le minimum est 0,25 cœur et 128 Mo. Le curseur parcourt une liste de valeurs permises : processeur par pas de 0,25 cœur ; mémoire par 128 Mo jusqu'à 2 Go, par 256 Mo jusqu'à 8 Go, par 1 Go au-delà, la mémoire totale étant toujours le dernier cran. La saisie à côté accepte une valeur exacte dans les mêmes bornes : tout nombre entier de Mo pour la mémoire, des multiples de 0,25 seulement pour le processeur (virgule ou point). Le fil reste en octets et en millicœurs ; Mo, Go et cœurs ne sont que l'affichage. Moins de 1 024 Mo s'écrit en Mo, sinon en Go (deux décimales au plus, multiples de 1 024).

La partie « fin de "Sans limite" » de la règle d'origine est remplacée par `BR-RES-037`.

## Application (code)
- `apps/desktop/src/resources/scale.ts::{memoryStops, cpuStops, nearestStopIndex, memoryInRange, cpuInRange}`
- `apps/desktop/src/resources/format.ts::{formatMemory, formatCpu, memoryFromField, cpuFromField, memoryFieldOf}`
- `apps/desktop/src/resources/capacity.ts::engineCapacityFromWire` (lecture des bornes)
- `apps/desktop/src/components/molecules/ResourceSlider.vue`

## Vérification
- `apps/desktop/src/resources/resources.test.ts` (crans, unités, bornes, lecture de la capacité)
- `apps/desktop/src/components/organisms/resourceSliders.test.ts` (saisie exacte, refus sur place)

## Cas limites
- Machine sous 128 Mo : un seul cran, 128 Mo.
- Mémoire totale qui n'est pas un nombre entier de Mo : arrondie vers le bas, c'est le dernier cran.
- Saisie en Go qui n'est pas un nombre entier de Mo (1,0001 Go) : refusée avec la phrase des bornes.

## Règles liées
- BR-RES-037, BR-RES-039

## Historique
- 2026-10-17 — HRT-146 : côté client (composant `ResourceSliders`).
