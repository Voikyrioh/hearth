---
id: FIX-01M5A5VH5GR2M9PF9Y1SB04D65
titre: L'action lancée par le compte connecté s'affichait « déjà en cours, lancée par marie » (HRT-65)
date_découverte: 2026-10-19
date_correction: 2026-10-19
---

# FIX-01M5A5VH5GR2M9PF9Y1SB04D65 : L'action lancée par le compte connecté s'affichait « déjà en cours, lancée par marie » (HRT-65)

## Symptôme
Juste après avoir démarré un service, le détail disait « Une action est déjà en cours sur ce service. Lancée par marie. » : lu comme si quelqu'un d'autre avait devancé le compte connecté (marie).

## Reproduction
`apps/desktop/src/pages/ServicesLifecycle.test.ts` (« l'action lancée par le compte connecté se dit « par vous » »), `src/services/transition.test.ts`, `e2e/hrt65-lifecycle.spec.ts`.

## Cause root
`launchedBy` et `pilotLock` ne connaissaient pas le compte connecté ; la phrase du verrou (BR-SVC-031, BR-LIFE-013) était la même pour tous.

## Impacté
Détail, raison sous les boutons, bandeau « Opérations en cours ».

## Workaround
Aucun.

## Correction
`launchedBy(compte, soi)` rend « Lancée par vous. » pour son propre compte ; `pilotLock(service, running, soi)` porte `own` ; `busyText` dit « Une action est en cours sur ce service. » (sans « déjà ») pour sa propre action. Le refus d'agent `service_busy` garde le nom du compte (le client n'y sait pas qui est « soi » à cet endroit).

## Références
- Ticket : HRT-65, PR #108 ; revue UX Nora, constat 2 ; BR-SVC-031, BR-LIFE-013
