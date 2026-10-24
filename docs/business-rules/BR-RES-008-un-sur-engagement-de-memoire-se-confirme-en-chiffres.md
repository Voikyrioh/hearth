---
id: BR-RES-008
domaine: RES
titre: Une limite de mémoire au-delà de ce qui reste disponible se confirme en chiffres avant le lancement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-022), HRT-102
maj: 2026-10-24
---

# BR-RES-008

## Règle
Règle portée par le client (HRT-102) pour l'installation d'une fiche. Si la limite choisie dépasse la mémoire disponible (mémoire totale, moins la réserve, moins les limites des services en marche), l'étape « Ressources » l'avertit en direct et la fenêtre « Vérifiez avant de lancer » demande une case « Je comprends » avec les deux chiffres. La case ne part pas à l'agent : c'est une confirmation du client.

## Application (code)
- `apps/desktop/src/fiche/attention.ts::attentionPoints`
- `apps/desktop/src/fiche/fit.ts::availableMemory`

## Vérification
- `apps/desktop/src/fiche/fiche.test.ts::les points d'attention (BR-FICHE-026)`

## Cas limites
- Le même calcul sert l'avertissement en direct et la case de la fenêtre.

## Règles liées
BR-FICHE-022, BR-FICHE-026

## Historique
- 2026-10-24 — création (HRT-102).
