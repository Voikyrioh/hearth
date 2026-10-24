---
id: BR-FICHE-059
domaine: FICHE
titre: Un jeton de serveur est un secret saisi par l'utilisateur, masqué, avec l'adresse où le créer
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-059), HRT-102
maj: 2026-10-24
---

# BR-FICHE-059

## Règle
Règle portée par le client (HRT-102). Une question de genre secret et de nature « jeton de serveur » est rendue par un champ masqué (« Afficher / Masquer »). La page dit que le jeton est propre à ce serveur de jeu, qu'il ne donne pas accès au compte et qu'il se révoque sur le site, affiche l'adresse de la fiche en entier et propose de l'ouvrir dans le navigateur de l'utilisateur ; Hearth ne la contacte jamais. Un jeton obligatoire laissé vide bloque « Continuer » (« Ce jeu a besoin d'un jeton de serveur. »). Aucun champ de nom d'utilisateur ou de mot de passe de compte n'existe nulle part (BR-FICHE-011).

## Application (code)
- `apps/desktop/src/components/molecules/QuestionField.vue`
- `apps/desktop/src-tauri/src/service_ops/catalog.rs::link_of`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::le jeton de serveur (BR-FICHE-059)`

## Cas limites
- La nature du secret (`secret_kind`) vient de la fiche : le client n'en devine aucune.

## Règles liées
BR-FICHE-011, BR-FICHE-034

## Historique
- 2026-10-24 — création (HRT-102).
