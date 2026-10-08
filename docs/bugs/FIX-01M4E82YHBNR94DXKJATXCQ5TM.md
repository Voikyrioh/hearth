---
id: FIX-01M4E82YHBNR94DXKJATXCQ5TM
titre: Journal jamais chargé hors ligne : un cadre vide sous une estampille (C46)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E82YHBNR94DXKJATXCQ5TM : Journal jamais chargé hors ligne : un cadre vide sous une estampille (C46)

## Symptôme
Journal jamais lu et serveur hors ligne : sous la carte de filtres, rien, et « Vu il y a… » en coin alors que rien n'a été vu.

## Reproduction
`pages/notLoadedYet.test.ts` « Journal jamais lu, serveur hors ligne… » (rouge avant).

## Cause root
Le message d'échec du Journal n'était plus montré hors « Connecté » (HRT-38, C30) et la page n'appelait pas `usePageData` : ni tableau, ni écran vide (`failed`), ni message, et le gabarit datait la page.

## Impacté
Page Journal d'activité, démarrage de l'application avec un serveur éteint. Source : review de la PR #57.

## Workaround
Aucun.

## Correction
« Pas encore chargé, sera disponible quand le serveur reviendra. » (phrase par état du lien) à la place, une seule fois ; `usePageData` branché : pas d'estampille tant que rien n'a été lu. `audit.reloadManually` retiré avec ses textes (plus d'appelant). 

## Règles
- BR-AUDIT-020 mise à jour (le geste de rechargement est celui du bandeau).
