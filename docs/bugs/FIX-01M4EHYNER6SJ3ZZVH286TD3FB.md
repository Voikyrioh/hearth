---
id: FIX-01M4EHYNER6SJ3ZZVH286TD3FB
titre: Journal : en-têtes décalés de 4 à 10 px de leurs colonnes avec une barre de défilement classique (C31)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYNER6SJ3ZZVH286TD3FB : Journal : en-têtes décalés de 4 à 10 px de leurs colonnes avec une barre de défilement classique (C31)

## Symptôme
Avec la barre de défilement classique de Windows, « ACTION », « CIBLE », « RÉSULTAT » et « RAISON » étaient 4 à 10 px à droite de leurs valeurs.

## Reproduction
`e2e/hrtp2.spec.ts` « chaque en-tête est au bord gauche de sa colonne (barres classiques) » aux 5 tailles : Playwright masque les barres par défaut (`--hide-scrollbars`), ce qui cachait le défaut ; le test les rétablit. Rouge avant à 1366, 1920 et 2560 (jusqu'à 7 px).

## Cause root
La zone de lignes perdait la largeur de sa barre (15 px) mais pas l'en-tête : les colonnes en `fr` se répartissaient sur deux largeurs différentes.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Même gouttière réservée des deux côtés (`scrollbar-gutter: stable`, en-tête en `overflow: hidden`). `// FIX:01M4EHYNER6SJ3ZZVH286TD3FB`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/organisms/AuditTable.vue`
