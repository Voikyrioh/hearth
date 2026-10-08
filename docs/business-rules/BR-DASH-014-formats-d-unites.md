---
id: BR-DASH-014
domaine: DASH
titre: Les nombres suivent les unités : pourcentages entiers, Go à une décimale, débit adaptatif, durée longue
statut: partielle
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-tableau-de-bord-machine.md (BR-DASH-014), technique-socle §2, §3, §7, §10, HRT-06
maj: 2026-10-08
---

# BR-DASH-014 — Les nombres suivent les unités : pourcentages entiers, Go à une décimale, débit adaptatif, durée longue

## Règle
Côté agent : valeurs brutes et stables, sans mise en forme : charges en pourcentage (0 à 100), quantités en octets, débits en octets par seconde, températures en degrés Celsius, durée de fonctionnement en secondes. Le formatage (entiers, Go à une décimale, Ko/s ou Mo/s, `3 j 4 h 12 min`) est l'affaire du client (HRT-11).

## Application (code)
- `crates/hearth-proto/src/api/metrics.rs` (unités documentées en tête de module).

## Vérification
- `hearth_proto::api::metrics::tests`

## Cas limites
- Aucun arrondi côté agent : le client arrondit à l'affichage seulement. Seule exception : une décimale sur les pourcentages et températures, par TRONCATURE (`trunc1`), la même règle que l'affichage (« 99 % » pour 99,96) : arrondir faisait classer et afficher « 100 % » une mesure à 99,96 (FIX-01M4CRD4NX3A34B7Z31A7RWE1B).

## Règles liées
- BR-DASH-002

## Interface
- `dashboard/format.ts` (gabarits et unités dans `i18n/fr.ts`) : `formatPercent` (entier TRONQUÉ : « 85 % » ne s'affiche que si le seuil de 85 est atteint), `formatGb`, le SEUL formateur de quantités (mémoire et disques) : tout en BINAIRE, comme l'Explorateur Windows, libellés « Go » et « To », 1 To = 1 024 Go (un octet = 1/1024³ Go) ; une décimale à VIRGULE française, sans « ,0 » (« 64 Go », « 12,5 Go ») ; bascule en To dès 1 024 Go ARRONDIS (« 1 023 Go », « 1 To », « 1,5 To », « 4 To » pour 4 096 Go ; jamais « 1024 Go ») ; un disque de 4 × 10¹² octets s'écrit « 3,6 To » comme chez Windows. Décision de Claude du 2026-10-08, à confirmer par Voiky. FIX-01M4DPR4FFVCMZN0KPEA4JVCC8, `formatRate` (Go/s, Mo/s avec virgule, Ko/s, o/s), `formatUptime` (3 j 4 h 12 min, 2 h 30 min). Tests : `dashboard/format.test.ts`.

## Historique
- 2026-10-04 — création (HRT-06, session 2026-10-04-hearth-creation).
- 2026-10-05 — interface du tableau de bord (HRT-11, session 2026-10-04-hearth-creation).
- 2026-10-08 — virgule française, sans « ,0 », To dès 1 024 Go, tout binaire (HRT-46, C12 ; décision de Claude du 2026-10-08, à confirmer par Voiky).
