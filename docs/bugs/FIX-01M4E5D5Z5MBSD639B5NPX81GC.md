---
id: FIX-01M4E5D5Z5MBSD639B5NPX81GC
titre: Étape « Empreinte » : quel serveur, où relire l'empreinte, que fait « Refuser » (C3)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D5Z5MBSD639B5NPX81GC : Étape « Empreinte » : quel serveur, où relire l'empreinte, que fait « Refuser » (C3)

## Symptôme
Huit groupes de lettres sans nom ni adresse du serveur, pas de « Précédent », rien sur où retrouver l'empreinte ni sur la conséquence d'un refus.

## Reproduction
`connect.test.ts` (étape empreinte, « Précédent »), `e2e/hrt38-40.spec.ts` (rouges avant).

## Cause root
L'étape ne rappelait rien de la saisie de l'étape 1.

## Impacté
Assistant d'ajout d'un serveur, étape 2. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Nom et adresse du serveur rappelés, bouton « Précédent » (retour à l'adresse, rien enregistré), phrase « Pour la relire plus tard, lance « sudo hearth-agent fingerprint » sur le serveur. » (commande du runbook d'installation), phrase sur « Refuser » : il annule l'ajout, rien n'est enregistré. Écart avec le ticket, qui disait que « Refuser » revient à l'étape précédente : le code abandonne l'ajout, le texte dit la vérité et « Précédent » fait le retour. Décision de Claude, à confirmer par Voiky.

## Règles
- Aucune règle métier touchée (interface).
