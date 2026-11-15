---
id: BR-MOD-006
domaine: MOD
titre: L'installation propose « Installer avec un pack de mods » ; le genre du pack est lu par l'agent, jamais saisi
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-006), HRT-125
maj: 2026-11-15
---

# BR-MOD-006 : L'installation propose « Installer avec un pack de mods » ; le genre du pack est lu par l'agent, jamais saisi

## Règle
Quand la fiche offre le dépôt d'un pack (`FicheDetail.mods`, BR-MOD-001), l'étape « Réponses » montre la carte « Mods » : la case « Installer avec un pack de mods » (jamais cochée d'avance), le choix du fichier, puis ce que l'agent a LU dans le contenu (`POST /deployments/pack-check`) : le genre (« Pack serveur » ou « Pack Modrinth », affiché, jamais un champ à remplir), les nombres et, pour un pack Modrinth, le nom, la version et ce que le pack annonce. Le moteur de serveur et la version de Minecraft sont des réponses de l'installation : pour un pack serveur, la carte dit où les lire (la page du pack) ; quand la fiche ne permet qu'un moteur pour le genre lu, il est choisi d'office. Cocher la case sans fichier, un fichier pas encore lu, un pack refusé, un genre que la fiche ne prend pas ou un moteur non permis pour ce genre empêchent de continuer (message en une phrase). Un pack refusé à l'installation est retiré de la zone d'attente de l'agent : il est à choisir de nouveau. L'envoi du fichier lui-même passe par les transferts reprenables (HRT-95, HRT-96) ; tant que ce client ne les a pas, la carte le dit.

## Application (code)
- `apps/desktop/src/fiche/pack.ts::{packIssue, allowedVariants, readPack}`
- `apps/desktop/src/composables/useFicheInstall.ts::{choosePack, setPackWanted}`
- `apps/desktop/src/components/organisms/InstallPack.vue`

## Vérification
- apps/desktop/src/fiche/pack.test.ts
- apps/desktop/src/pages/InstallPack.test.ts : `montre le genre lu par l'agent et ce qu'il a compté, jamais un champ à remplir`, `choisit d'office le seul moteur que la fiche permet pour le genre du pack`, `refuse de continuer quand le moteur choisi ne convient pas au genre du pack`

## Cas limites
- Annuler le choix du fichier ne dit rien.
- Le moteur de serveur d'un service installé est figé : le remplacement d'un pack après l'installation est HRT-217.

## Règles liées
- BR-MOD-007, BR-MOD-008, BR-MOD-009, BR-MOD-016

## Historique
- 2026-11-15 : création (HRT-125).
