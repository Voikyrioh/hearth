---
id: FIX-01M5PYVP4V48YGH36AYS43Q31V
titre: Le message d'un caractère refusé s'arrêtait sur deux points quand le caractère était une espace (HRT-102)
date_découverte: 2026-10-24
date_correction: 2026-10-24
---

# FIX-01M5PYVP4V48YGH36AYS43Q31V : Le message d'un caractère refusé s'arrêtait sur deux points quand le caractère était une espace (HRT-102)

## Symptôme
Nom de monde « Mon monde » dans l'installation d'un jeu : « Ce texte contient un caractère que ce jeu ne sait pas lire : » puis rien. Le caractère fautif est une espace, invisible. Même défaut pour toute espace insécable, tabulation ou caractère de largeur nulle.

## Cause root
Le message citait le caractère tel quel (`{character}`). Un caractère qu'on ne voit pas ne dit rien. Rien dans le message ne disait quoi corriger.

## Reproduction
Installer Terraria depuis le Catalogue, écrire « Mon monde » dans « Nom du monde », quitter le champ : le message s'arrête sur « : ». Tests : le scénario « curseur dans le nom, caractère refusé en mots… » de `apps/desktop/e2e/ux-2026-10-24.spec.ts` et `apps/desktop/src/fiche/characters.test.ts`.

## Impacté
`fiche/messages.ts::charsetText`, `fiche/character.ts`, `fiche/answers.ts`, `settings/messages.ts` (refus d'un réglage).

## Workaround
Aucun : deviner que l'espace est refusée.

## Correction
Le caractère est nommé en mots (« l'espace », « le point d'exclamation », textes dans `fr.ts` `fiche.character.names`, couverts par `copy.test.ts`) ; un caractère invisible se nomme par son code (« le caractère invisible U+200B »). Pour un texte non secret, le client propose le texte corrigé (« Essayez « Mon_monde ». », `correctedText`). Jamais de caractère ni de correction pour un secret, quel que soit son genre (`question.secret`). BR-FICHE-017 mise à jour.

## Références
- Revue UX de Nora du 24/10, constat 5 ; ticket : HRT-102
- ADR / BR : BR-FICHE-017
- Code : `apps/desktop/src/fiche/messages.ts` (marqueur `FIX:01M5PYVP4V48YGH36AYS43Q31V`)
