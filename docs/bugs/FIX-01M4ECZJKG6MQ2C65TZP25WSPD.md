---
id: FIX-01M4ECZJKG6MQ2C65TZP25WSPD
titre: La fenêtre de retrait d'un poste alignait quatre paragraphes avant le champ (C39)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4ECZJKG6MQ2C65TZP25WSPD : La fenêtre de retrait d'un poste alignait quatre paragraphes avant le champ (C39)

## Symptôme
Question, explication, aide du mot de passe et conseil : quatre paragraphes avant le champ du mot de passe.

## Reproduction
Vitest `pages/SecurityMode.test.ts` « Retrait d'un poste » : un seul paragraphe avant le champ (les deux phrases de la spec), le conseil replié. Rouge avant : trois paragraphes visibles.

## Cause root
Les textes s'étaient ajoutés au fil des lots sans hiérarchie.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Un paragraphe (les deux phrases de la spec), le champ, puis le conseil « Si ce poste n'est plus à toi » replié. `// FIX:01M4ECZJKG6MQ2C65TZP25WSPD`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Tickets : HRT-39, HRT-40, HRT-41, HRT-43 (revue UX du 2026-10-08)
- Code : `components/organisms/RemoveDeviceDialog.vue`
