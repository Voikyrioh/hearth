---
id: FIX-01M4EPX99456BPK54R5JHE4JPA
titre: Les textes d'exemple de l'ajout d'un serveur étaient copiés de la machine de Voiky (S4)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EPX99456BPK54R5JHE4JPA : Les textes d'exemple de l'ajout d'un serveur étaient copiés de la machine de Voiky (S4)

## Symptôme
« Forge » et « 192.168.1.20 ou forge.maison » en exemples.

## Reproduction
`i18n/plainLanguage.test.ts` « textes d'exemple neutres » : aucun texte de l'interface ne contient « forge », l'adresse 192.168.1.20 ou « Voiky ». Rouge avant.

## Cause root
Les textes d'exemple reprenaient les valeurs de la forge.

## Impacté
Le client, vu par Voiky lors de son premier smoke sur la forge (HRT-47).

## Workaround
Aucun.

## Correction
« ex. salon » et « ex. 192.168.1.10 ou serveur.local ». `// FIX:01M4EPX99456BPK54R5JHE4JPA`.

## Règles
- Aucune règle métier modifiée (la liste des disques côté agent est S1a, autre tâche).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-47 ; captures `contexts/hearth/art/smoke-2026-10-08/`
- Code : `i18n/fr.ts`
