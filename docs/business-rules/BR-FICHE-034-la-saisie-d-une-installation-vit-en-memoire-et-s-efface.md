---
id: BR-FICHE-034
domaine: FICHE
titre: La saisie d'une installation, mot de passe de jeu compris, vit en mémoire seulement, jusqu'à l'issue ou l'abandon
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-034), HRT-102
maj: 2026-10-24
---

# BR-FICHE-034

## Règle
Règle portée par le client (HRT-102). Fermer le formulaire quand une réponse est saisie demande « Abandonner votre saisie ? ». La saisie n'est jamais écrite sur le disque du PC, ni dans l'adresse, ni dans un instantané : elle vit dans un conteneur mémoire effaçable (`stores/ficheInstall.ts`) jusqu'à ce que l'issue de l'installation soit connue, puis est effacée (valeurs écrasées avant d'être lâchées), ou aussitôt à l'abandon. Une fiche qui change de version ou d'empreinte pendant la saisie repart d'un brouillon neuf. Le mot de passe de confirmation traverse l'appel sans être gardé.

## Application (code)
- `apps/desktop/src/stores/ficheInstall.ts`
- `apps/desktop/src/fiche/draft.ts::wipeFicheDraft`
- `apps/desktop/src/pages/InstallFromFiche.vue`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::fermer et abandonner (BR-FICHE-034, BR-DEPLOY-029)`
- `apps/desktop/src/fiche/fiche.test.ts::efface la saisie, secrets compris`

## Cas limites
- JavaScript ne permet pas d'effacer une chaîne : les valeurs sont écrasées puis les références lâchées.

## Règles liées
BR-DEPLOY-029, BR-FICHE-014

## Historique
- 2026-10-24 — création (HRT-102).
