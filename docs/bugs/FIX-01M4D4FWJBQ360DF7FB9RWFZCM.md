---
id: FIX-01M4D4FWJBQ360DF7FB9RWFZCM
titre: « Demander mon mot de passe » : le sélecteur se cassait sur trois lignes (C33)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FWJBQ360DF7FB9RWFZCM : « Demander mon mot de passe » : le sélecteur se cassait sur trois lignes (C33)

## Symptôme
« Toutes les / 5 minutes » et « À / chaque / action » empilés dans un bloc étroit.

## Reproduction
`e2e/layout.spec.ts` « sécurité à 1280 et 1920 » : chaque choix tient sur une ligne (au plus 44 px) ; rouge avant.

## Cause root
Le contrôle était écrasé par le texte d'aide de la ligne de réglage et ses libellés pouvaient passer à la ligne.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
`white-space: nowrap` sur les choix, contrôle qui ne rétrécit pas, colonne du texte qui rétrécit. `// FIX:01M4D4FWJBQ360DF7FB9RWFZCM`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/atoms/HSegmented.vue`, `molecules/SettingRow.vue`
