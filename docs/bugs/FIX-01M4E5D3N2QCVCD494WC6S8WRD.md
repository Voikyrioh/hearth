---
id: FIX-01M4E5D3N2QCVCD494WC6S8WRD
titre: Action impossible : la fenêtre garde sa question et un bouton grisé, sans chemin (C40, C41)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D3N2QCVCD494WC6S8WRD : Action impossible : la fenêtre garde sa question et un bouton grisé, sans chemin (C40, C41)

## Symptôme
« Supprimer le compte ? » en titre, « Mets à jour l'agent. » ou « Me reconnecter pour enregistrer ce poste » au milieu, un bouton d'action grisé, aucun chemin vers les réglages.

## Reproduction
`adminAct.test.ts`, `e2e/hrt38-40.spec.ts`, `e2e/reauth.spec.ts` (rouges avant).

## Cause root
`AdminActDialog` n'adaptait que le contenu, pas le titre ni les boutons, quand l'agent est trop ancien ou le poste sans clé.

## Impacté
Toutes les fenêtres d'acte d'administration (comptes, mode attaque, mise à jour). Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Titre de l'état vrai (« Mise à jour requise », « Poste non enregistré »), plus de bouton d'action (`FormDialog` : `hideSubmit`), « Aller aux réglages » pour l'agent trop ancien, « Me reconnecter pour enregistrer ce poste » pour le poste sans clé ; « Fermer » reste pour quitter (décision de Claude, à confirmer par Voiky : le ticket disait que les autres boutons disparaissent).

## Règles
- Aucune règle métier touchée (interface).
