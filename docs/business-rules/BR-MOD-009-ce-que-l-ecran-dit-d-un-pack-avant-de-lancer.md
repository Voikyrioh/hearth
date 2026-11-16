---
id: BR-MOD-009
domaine: MOD
titre: Avant de lancer, l'écran dit ce que le programme du jeu fera du pack, et finit toujours par la phrase sur les mods
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-009), HRT-125
maj: 2026-11-15
---

# BR-MOD-009 : Avant de lancer, l'écran dit ce que le programme du jeu fera du pack, et finit toujours par la phrase sur les mods

## Règle
Dans « Vérifiez avant de lancer », une ligne « Pack de mods » dit le genre lu par l'agent et : pour un pack Modrinth, « Ce pack fait télécharger {n} fichiers par le programme du jeu, depuis {hôtes}. Hearth ne les vérifie pas. » ; pour un pack serveur, « Ce pack contient {n} mods. Le programme du jeu copie son contenu dans son dossier. Hearth ne l'ouvre pas et ne le vérifie pas. » ; et TOUJOURS, en dernier : « Un mod est un programme écrit par quelqu'un d'autre. Il tourne avec les droits du serveur de jeu. ». Aucune phrase ne parle d'une clé (aucune voie CurseForge, BR-MOD-017). Les nombres et les hôtes sont ceux que l'agent a lus, les textes du pack sont nettoyés.

## Application (code)
- `apps/desktop/src/fiche/pack.ts::packSentences`
- `apps/desktop/src/components/organisms/FicheReview.vue`

## Vérification
- apps/desktop/src/fiche/pack.test.ts : `dit avant de lancer ce que le jeu fera, et finit toujours par la phrase des mods`

## Cas limites
- « Hearth télécharge {n} fichiers depuis Modrinth » (téléchargement par Hearth) est HRT-126.

## Règles liées
- BR-MOD-006, BR-MOD-007, BR-MOD-017

## Historique
- 2026-11-15 : création (HRT-125).
