---
id: BR-MOD-024
domaine: MOD
titre: Retirer un mod ne supprime rien : le fichier va dans « Mods retirés »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-024 : Retirer un mod ne supprime rien

## Règle
« Retirer » déplace le fichier (`.jar` ou `.jar.disabled`) de `data/mods` dans `data/mods-retires` du même service, par un renommage SANS remplacement ; ce dossier n'est jamais lu par le jeu et fait partie des sauvegardes. Un nom déjà pris dans « Mods retirés » reçoit un numéro (`-2` à `-99`, le premier libre). Un fichier remplacé par un dépôt part de la même façon. Hearth ne vide jamais « Mods retirés ». « Remettre » est le geste inverse (un nom déjà pris dans `data/mods` refuse le lot : `MOD_NAME_TAKEN`). Un mod listé par le manifeste du pack Modrinth ne se retire, ne se remplace et ne se désactive pas (`MOD_IN_MODRINTH_PACK`) : le serveur le remettrait à chaque démarrage. Un nom absent du dossier visé : `MOD_FILE_MISSING`. Aucun geste du lot ne supprime un fichier.

## Application (code)
- Client (HRT-262) : `apps/desktop/src/services/mods.ts::gestureGate`, `composables/useServiceMods.ts` (lot), `ModRow.vue`
- `crates/hearth-agent/src/domain/mods/apply.rs::plan` (choix du nom libre, refus, ordre des gestes)
- `crates/hearth-agent/src/application/ports/safe_fs.rs::ServiceRoot::rename_noreplace` (le seul geste de déplacement : `renameat2` `RENAME_NOREPLACE`, aucun lien suivi, aucun montage franchi)

## Vérification
- `domain/mods/apply.rs::tests` (nom libre, fichier absent, nom pris, pack Modrinth)
- `tests/mods_apply_https.rs` (rien n'est supprimé : l'ancien et le retiré sont dans « Mods retirés », intacts)

## Règles liées
- BR-MOD-025, BR-MOD-027, BR-MOD-032, BR-MOD-028

## Historique
- 2026-11-18 : création (HRT-258).
