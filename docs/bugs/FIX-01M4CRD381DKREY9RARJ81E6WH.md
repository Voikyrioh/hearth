---
id: FIX-01M4CRD381DKREY9RARJ81E6WH
titre: Une vue sans échantillon (ou plus ancienne) remplaçait l'identité de la machine reçue à la connexion
date_découverte: 2026-10-06
date_correction: 2026-10-08
---

# FIX-01M4CRD381DKREY9RARJ81E6WH : Une vue sans échantillon (ou plus ancienne) remplaçait l'identité de la machine reçue à la connexion

## Symptôme
L'identité affichée (nom, système, disques) pouvait revenir à celle d'une session précédente après une lecture du disque tardive ou une vue sans échantillon.

## Reproduction
`stores/dashboard.test.ts` : « a view without any sample (identity only) does not overwrite the identity of a live snapshot » ; rouge avant (la vue vide écrasait).

## Cause root
`apply` ne jugeait une vue « périmée » que si elle avait un dernier échantillon ET que `latest` était posé : une vue sans échantillon n'était jamais périmée, et la comparaison se faisait à `latest` et non à tout ce que l'anneau connaît (l'heure lue à l'ouverture, par exemple).

## Impacté
Le tableau de bord (HRT-11, PR #14), jamais publié.

## Workaround
Aucun.

## Correction
La vue est périmée quand une identité est déjà connue et que son dernier échantillon est plus ancien que le plus récent déjà connu de l'anneau, ou qu'elle n'en a aucun. Sans identité connue, toute vue est prise. `// FIX:01M4CRD381DKREY9RARJ81E6WH`.

## Règles
- BR-DASH-011 (note).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suites de la review HRT-11, rounds 2 et 3)
- Code : `apps/desktop/src/stores/dashboard.ts`
