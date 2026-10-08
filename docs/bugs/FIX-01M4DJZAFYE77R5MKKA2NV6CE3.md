---
id: FIX-01M4DJZAFYE77R5MKKA2NV6CE3
titre: Le mode attaque éteint parlait au présent et en jargon (C34)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DJZAFYE77R5MKKA2NV6CE3 : Le mode attaque éteint parlait au présent et en jargon (C34)

## Symptôme
Mode éteint, la carte disait « Seuls les postes reconnus peuvent se connecter. Un poste connu par un seul signe a droit à un essai. » (même phrase éteint ou actif) ; actif, trois paragraphes de règles internes.

## Reproduction
`SecurityMode.test.ts` « shows the card inactive… », « says what is happening in one sentence when active… » ; Playwright `hrt39-security.spec.ts`.

## Cause root
Un seul texte pour les deux états, rédigé comme une règle interne.

## Impacté
Page Sécurité du client Windows depuis HRT-26. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Éteint : « Le mode attaque permet de ne laisser se connecter que les postes reconnus… » (futur, sans « signe »). Actif : une phrase (« est actif depuis {heure} : seuls les postes reconnus peuvent se connecter ») ; les règles sont dans un `<details>` « Comment ça marche », replié. La fenêtre de confirmation et la notification disent « à moitié reconnu… un seul essai ». `FIX:` dans `fr.ts`.

## Règles
- BR-TRUST-010, 018, 028, 029 (mode attaque), BR-TRUST-008, 009 (alerte).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-39
