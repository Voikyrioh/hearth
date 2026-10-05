---
id: BR-DASH-014
domaine: DASH
titre: Les nombres suivent les unités : pourcentages entiers, Go à une décimale, débit adaptatif, durée longue
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-014), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-05
---

# BR-DASH-014 — Les nombres suivent les unités : pourcentages entiers, Go à une décimale, débit adaptatif, durée longue

## Règle
Côté agent : valeurs brutes et stables, sans mise en forme : charges en pourcentage (0 à 100), quantités en octets, débits en octets par seconde, températures en degrés Celsius, durée de fonctionnement en secondes. Le formatage (entiers, Go à une décimale, Ko/s ou Mo/s, `3 j 4 h 12 min`) est l'affaire du client (HRT-11).

## Application (code)
- `crates/hearth-proto/src/api/metrics.rs` (unités documentées en tête de module).

## Vérification
- `hearth_proto::api::metrics::tests`

## Cas limites
- Aucun arrondi côté agent : le client arrondit à l'affichage seulement.

## Règles liées
- BR-DASH-002

## Interface
- `dashboard/format.ts` : `formatPercent` (entier), `formatGb` (Go à une décimale, puissances de 1024), `formatRate` (Go/s, Mo/s, Ko/s, o/s), `formatUptime` (3 j 4 h 12 min, 2 h 30 min). Tests : `dashboard/format.test.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
