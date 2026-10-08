---
id: FIX-01M4E5D4JY66T0ETEMRY1DZQZK
titre: Page jamais chargée hors ligne : erreur, deuxième « Réessayer » et « Vu il y a… » sans rien vu (C46)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D4JY66T0ETEMRY1DZQZK : Page jamais chargée hors ligne : erreur, deuxième « Réessayer » et « Vu il y a… » sans rien vu (C46)

## Symptôme
« Impossible de lire la liste des comptes. Réessayer » sous le bandeau « Réessayer maintenant » ; estampille « Vu il y a 12 s » alors qu'aucune donnée n'a été lue.

## Reproduction
`pages/notLoadedYet.test.ts`, `e2e/hrt38-40.spec.ts` (rouges avant).

## Cause root
Les écrans de comptes, d'état de sécurité et de postes montraient leur échec de lecture comme si le serveur répondait, et le gabarit datait la page même sans donnée.

## Impacté
Pages Comptes et Sécurité (cartes mode attaque et postes de confiance), gabarit du serveur. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
« Pas encore chargé, sera disponible quand le serveur reviendra. » sans bouton quand le lien n'est pas « Connecté » ; les pages disent au gabarit (`usePageData`) qu'elles n'ont rien lu, et il n'affiche alors pas d'estampille.

## Règles
- Aucune règle métier touchée (interface).
