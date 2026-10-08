---
id: FIX-01M4DPR45X80GAYPS8MJ4RPZ8E
titre: Barre des serveurs : outils sans nom, couleurs seulement sur le serveur ouvert, suppression sans nom (C49, C52)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DPR45X80GAYPS8MJ4RPZ8E : Barre des serveurs : outils sans nom, couleurs seulement sur le serveur ouvert, suppression sans nom (C49, C52)

## Symptôme
Survol de « + », « Mes serveurs » et « Réglages » : aucune infobulle. Seul le serveur ouvert montrait sa couleur. La fenêtre disait « Supprimer ce serveur ? ».

## Reproduction
`layout.spec.ts` « barre des serveurs à … » et « suppression d'un serveur à … » (rouges avant), `Servers.test.ts`.

## Cause root
Les trois liens de la barre n'avaient qu'un `aria-label`, la couleur n'était posée que sur l'anneau de l'avatar actif, le titre de la fenêtre était un texte fixe.

## Impacté
Barre des serveurs, avatars, carnet « Mes serveurs ». Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Bulle de nom visuelle (`aria-hidden`, `position: fixed` : la liste qui défile ne la rogne pas) à droite de chaque lien de la barre, outils et avatars ; un seul nom accessible par lien (étiquette ou avatar), ni `title` ni description, contour et teinte de la couleur sur tous les avatars, titre « Supprimer {name} ? ».

## Règles
- Aucune règle métier touchée (interface), sauf mention.
