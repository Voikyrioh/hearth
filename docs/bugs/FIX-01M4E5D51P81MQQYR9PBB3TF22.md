---
id: FIX-01M4E5D51P81MQQYR9PBB3TF22
titre: Accès révoqué : « Connecte-toi à forge » alors qu'on ne peut pas (C47)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D51P81MQQYR9PBB3TF22 : Accès révoqué : « Connecte-toi à forge » alors qu'on ne peut pas (C47)

## Symptôme
Titre « Connecte-toi à forge », « Ton compte n'est plus accessible. » sans cause, un seul bouton.

## Reproduction
`connect.test.ts` « explains a revoked access… », `e2e/hrt38-40.spec.ts` (rouges avant).

## Cause root
Le panneau de reconnexion gardait son titre d'invite à se connecter pour l'accès révoqué.

## Impacté
Panneau de reconnexion (accès révoqué). Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Titre « Accès révoqué sur {nom} », explication (compte supprimé ou désactivé, ou mot de passe changé : la bibliothèque ne les distingue pas, BR-RESIL-014), invitation à demander à l'administrateur de rétablir le compte, bouton « Utiliser un autre compte ». Décision de Claude, à confirmer par Voiky : pas de bouton « Contacter l'administrateur » (aucun moyen de contact n'existe dans l'application), une phrase à la place.

## Règles
- BR-RESIL-014 mise à jour.
