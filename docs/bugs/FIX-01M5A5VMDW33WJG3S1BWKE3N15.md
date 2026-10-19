---
id: FIX-01M5A5VMDW33WJG3S1BWKE3N15
titre: Avertissement Vue « attribut sans destinataire » à chaque ouverture d'une confirmation de pilotage (HRT-65)
date_découverte: 2026-10-19
date_correction: 2026-10-19
---

# FIX-01M5A5VMDW33WJG3S1BWKE3N15 : Avertissement Vue « attribut sans destinataire » à chaque ouverture d'une confirmation de pilotage (HRT-65)

## Symptôme
En développement, la console affichait à chaque ouverture de la confirmation d'un service un avertissement Vue (attribut non hérité par un composant). Sans effet visible.

## Reproduction
`apps/desktop/src/pages/ServicesLifecycle.test.ts` (« la fenêtre s'ouvre sans avertissement de Vue »), qui échoue si l'attribut revient.

## Cause root
`ServiceLifecycleDialog` passait `data-lifecycle-dialog` à `AdminActDialog`, dont la racine est une fenêtre téléportée : un attribut ne peut pas y être hérité. L'attribut n'était lu nulle part.

## Impacté
Console de développement.

## Workaround
Aucun.

## Correction
Attribut retiré, commentaire en place.

## Références
- Ticket : HRT-65, PR #108 ; revue UX Nora, « Vu en passant »
