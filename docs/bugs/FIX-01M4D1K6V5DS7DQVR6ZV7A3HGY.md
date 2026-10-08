---
id: FIX-01M4D1K6V5DS7DQVR6ZV7A3HGY
titre: Le titre « Durée de fonctionnement » se cassait en deux lignes et le point de montage des disques était presque invisible (C14)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D1K6V5DS7DQVR6ZV7A3HGY : Le titre « Durée de fonctionnement » se cassait en deux lignes et le point de montage des disques était presque invisible (C14)

## Symptôme
À 1280 et 1366 px le titre d'une carte passait sur deux lignes ; le point de montage « / » était gris sombre et collé au nom du disque.

## Reproduction
Le test de géométrie ci-dessus (titres sur une ligne à toutes les tailles).

## Cause root
Le titre de carte pouvait passer à la ligne ; le point de montage utilisait `--tx3` avec un espace de 8 px.

## Impacté
Le tableau de bord (HRT-11), jamais publié.

## Workaround
Aucun.

## Correction
Titre sur une ligne (`white-space: nowrap`, points de suspension en dernier recours) ; point de montage en `--tx2`, espacé de `--space-3`. `// FIX:01M4D1K6V5DS7DQVR6ZV7A3HGY`.

## Règles
- Design : grille 12 puis 6 colonnes (`contexts/hearth/conceptions/2026-10-04-design-ecrans-socle.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `apps/desktop/src/components/molecules/DashCard.vue`, `organisms/DisksCard.vue`
