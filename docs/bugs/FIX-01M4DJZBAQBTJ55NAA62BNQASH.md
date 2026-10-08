---
id: FIX-01M4DJZBAQBTJ55NAA62BNQASH
titre: Le menu disait « Sécurité », la page « Sécurité et mode attaque » (C39)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DJZBAQBTJ55NAA62BNQASH : Le menu disait « Sécurité », la page « Sécurité et mode attaque » (C39)

## Symptôme
Deux titres pour la même page.

## Reproduction
`Security.test.ts` (titre), `hrt39-security.spec.ts` « la page et le menu portent le même titre ».

## Cause root
Deux textes dans `fr.ts`.

## Impacté
Page Sécurité du client Windows depuis HRT-26. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Le titre de la page est « Sécurité », comme le menu. Reste de C39, NON fait : noms de postes (à vérifier sur un vrai agent), fenêtre de retrait à épurer.

## Règles
- Aucune règle métier touchée (interface).

## Non-régression
- Les deux tests ci-dessus.

## Références
- Ticket : HRT-39
