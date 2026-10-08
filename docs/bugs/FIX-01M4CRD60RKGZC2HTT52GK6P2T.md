---
id: FIX-01M4CRD60RKGZC2HTT52GK6P2T
titre: La série du processeur de la coquille et le repli des dates suivaient l'horloge murale du poste
date_découverte: 2026-10-06
date_correction: 2026-10-08
---

# FIX-01M4CRD60RKGZC2HTT52GK6P2T : La série du processeur de la coquille et le repli des dates suivaient l'horloge murale du poste

## Symptôme
Un saut de l'horloge murale (changement d'heure, synchronisation, réglage à la main) pouvait casser ou fausser la tenue de 30 s du niveau du processeur.

## Reproduction
Par construction : `now_ms()` ne recule plus. Le test `the_hour_before_the_snapshot_is_kept…` (tests/dashboard.rs) et les tests de la série passent un `now_ms` explicite.

## Cause root
`now_ms()` lisait `SystemTime::now()` à chaque appel.

## Impacté
Le tableau de bord (HRT-11, PR #14), jamais publié.

## Workaround
Aucun.

## Correction
`now_ms()` lit l'horloge murale UNE fois puis avance sur l'horloge monotone (`Instant`) : elle ne recule jamais. `// FIX:01M4CRD60RKGZC2HTT52GK6P2T`.

## Règles
- BR-DASH-004 (note).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suites de la review HRT-11, rounds 2 et 3)
- Code : `apps/desktop/src-tauri/src/link.rs`
