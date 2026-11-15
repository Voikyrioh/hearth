---
id: BR-MOD-012
domaine: MOD
titre: Hearth n'exécute jamais un mod ni un script de pack, et ne lit pas leur contenu
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-012), HRT-124
maj: 2026-11-15
---

# BR-MOD-012 : Hearth n'exécute jamais un mod ni un script de pack, et ne lit pas leur contenu

## Règle
Hearth ne lance aucun programme d'installation, aucun script d'un pack, et ne déballe aucune archive contenue dans une autre. Pour un pack il lit un seul fichier de description (1 Mio) ; le reste de l'archive est jugé (noms, types, tailles, sommes) sans être interprété, puis rangé tel quel en lecture seule. Le programme du jeu, dans son conteneur, fait le reste. Hearth ne garantit rien de ce qu'un mod fait dans le service.

## Application (code)
- `crates/hearth-agent/src/application/pack_intake.rs::OneFile`
- `crates/hearth-agent/src/domain/mods/pack.rs::judge_pack`

## Vérification
- crates/hearth-agent/src/domain/mods/pack.rs (tests)
- crates/hearth-agent/tests/pack_install_https.rs : le dossier des packs est monté en lecture seule

## Cas limites
- Un mod malveillant dont le jeu charge le code : hors de ce que Hearth peut juger (ADR-0054, limites).

## Règles liées
- BR-MOD-007, BR-ARCH-001

## Historique
- 2026-11-15 : création (HRT-124).
