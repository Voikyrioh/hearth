---
id: FIX-01M4E48N732694TTFSRQF1C4KG
titre: Mise à jour de l'agent : le bouton, l'étape et la mention se répètent (C43)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E48N732694TTFSRQF1C4KG : Mise à jour de l'agent : le bouton, l'étape et la mention se répètent (C43)

## Symptôme
Bouton « Mettre à jour l'agent » grisé sous la liste d'étapes ; étapes finies avec « … » ; étape dite deux fois ; « Mise à jour disponible » dite trois fois ; la pastille chevauchait le titre à 1100 px.

## Reproduction
`messages.test.ts`, `agentUpdate.test.ts`, `layout.spec.ts` « étape dite une fois » (rouges avant).

## Cause root
La carte n'avait pas de cas « en cours » pour le bouton, pas de libellé d'étape finie, une phrase d'avancement qui redisait l'étape et une phrase redisant la mention ; le titre en `flex: 1` se réduisait à rien.

## Impacté
Carte « État du serveur » des réglages. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Bouton retiré pendant la mise à jour, libellés d'étapes finies sans points, phrase d'avancement sans l'étape (l'étape courante reste annoncée aux lecteurs d'écran par une région vivante polie invisible), phrase « disponible pour l'agent » supprimée, titre en `flex: 1 1 auto`.

## Règles
- BR-UPDATE-013 mise à jour (phrase d'avancement, bouton retiré).
