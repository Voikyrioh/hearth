---
id: FIX-01M4EDC0FVT0ZHGASFEMWX7066
titre: Réglages et Sécurité : une colonne étroite, une colonne étirée, et du contenu sans borne à 1920 et 2560 (C44, C55 en partie)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EDC0FVT0ZHGASFEMWX7066 : Réglages et Sécurité : une colonne étroite, une colonne étirée, et du contenu sans borne à 1920 et 2560 (C44, C55 en partie)

## Symptôme
Réglages : trois cartes de 640 px à gauche, des cartes de 1 150 à 1 800 px presque vides à droite. Sécurité : un tiers de l'écran à 2560. Pages de travail et tableau des comptes sans borne de largeur.

## Reproduction
`e2e/hrt42.spec.ts` aux 5 tailles : à 1920 et 2560, colonnes de Réglages égales et bornées (600 px chacune), Sécurité en deux colonnes égales bornées à 1 520 px, contenu des pages de travail borné à 1 760 px, tableau des comptes à 1 400 px ; à 1100, 1280 et 1366, valeurs d'avant inchangées (colonne de 640 px, Sécurité sur une colonne de 880 px, toute la place). Rouge avant à 1920 et 2560, vert avant à 1100, 1280 et 1366.

## Cause root
`grid-template-columns: 640px 1fr` pour Réglages ; largeur fixe de 880 px pour Sécurité ; aucune borne pour les autres écrans.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Jetons `--page-max`, `--table-max`, `--page-max-settings`, `--page-max-security`. Réglages et Sécurité : deux colonnes à partir de 1 500 px de PAGE (requêtes de conteneur, pas de largeur de fenêtre ; 1 612 et 1 820 px de fenêtre aujourd'hui). La note « Effacement en attente » reste après la confirmation du mot de passe, à toutes les largeurs (en deux colonnes : sous elle, colonne de droite). Pas d'échelle automatique 125/150 % (attend l'accord de Voiky). Décision de Claude, à confirmer par Voiky. `// FIX:01M4EDC0FVT0ZHGASFEMWX7066`.

## Règles
- Aucune règle métier.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-42 ; conception `contexts/hearth/conceptions/2026-10-08-design-grand-ecran.md`
- Code : `pages/Settings.vue`, `pages/Security.vue`, `layouts/ServerLayout.vue`, `components/organisms/AccountTable.vue`
