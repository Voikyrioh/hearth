---
id: FIX-01M4EHYPD658X22RH4SFV0TDVE
titre: Comptes : Échap sur « Changer le rôle » laissait le curseur nulle part (D6)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYPD658X22RH4SFV0TDVE : Comptes : Échap sur « Changer le rôle » laissait le curseur nulle part (D6)

## Symptôme
Après Échap dans la liste du rôle, plus aucun élément n'avait le curseur.

## Reproduction
`e2e/hrtp2.spec.ts` « Échap sur « Changer le rôle » rend le curseur au bouton ». Rouge avant.

## Cause root
La cellule d'édition était retirée avec le focus dedans.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Échap rend le curseur au bouton d'origine. Non traité (noté dans HRT-44) : la première flèche dans la liste valide tout de suite (comportement natif de la liste) ; le focus de l'étape « Empreinte » de l'ajout d'un serveur. `// FIX:01M4EHYPD658X22RH4SFV0TDVE`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-44 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/organisms/AccountTable.vue`
