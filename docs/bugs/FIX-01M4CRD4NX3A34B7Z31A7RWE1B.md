---
id: FIX-01M4CRD4NX3A34B7Z31A7RWE1B
titre: Les mesures de l'agent étaient arrondies à la décimale alors que l'affichage tronque (99,96 devenait 100 %)
date_découverte: 2026-10-06
date_correction: 2026-10-08
---

# FIX-01M4CRD4NX3A34B7Z31A7RWE1B : Les mesures de l'agent étaient arrondies à la décimale alors que l'affichage tronque (99,96 devenait 100 %)

## Symptôme
Une charge à 99,96 % était envoyée à 100,0 : classée et affichée « 100 % » là où le client tronque (« 99 % »).

## Reproduction
`infrastructure::system::gpu::tests::truncation_keeps_one_decimal_like_the_display` ; rouge avec `round`.

## Cause root
`round1` arrondissait (`(v * 10).round() / 10`) ; l'affichage (`Math.floor`) tronque.

## Impacté
Le tableau de bord (HRT-11, PR #14), jamais publié.

## Workaround
Aucun.

## Correction
`round1` devient `trunc1` : troncature vers le bas avec une tolérance de 1e-4 qui absorbe le bruit des flottants (0,7 lu 0,69999999 reste 0,7). `// FIX:01M4CRD4NX3A34B7Z31A7RWE1B`.

## Règles
- BR-DASH-014 (exception dite).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suites de la review HRT-11, rounds 2 et 3)
- Code : `crates/hearth-agent/src/infrastructure/system/gpu/mod.rs`
