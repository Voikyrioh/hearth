---
id: BR-MOD-019
domaine: MOD
titre: Le jeu et le moteur d'un pack serveur sont lus dans le pack ; « LATEST » ne remplace jamais ce que le pack annonce, une version qui le contredit est refusée
statut: active
invariant: false
source: HRT-225 (smoke du 16/11/2026, Fabric 1.20.1 lancé en LATEST), BR-MOD-007
maj: 2026-11-16
---

# BR-MOD-019 : Le jeu et le moteur d'un pack serveur sont lus dans le pack

## Règle
À l'installation d'une fiche avec un pack serveur (fiche dont un réglage `version` lit `VERSION`) :
1. L'agent lit dans l'archive, sans rien déballer, la version de Minecraft et le moteur : `manifest.json` (CurseForge), `variables.txt` (ServerStarter), `install.properties` (installateur Fabric), puis les dossiers `libraries/net/fabricmc/{intermediary,fabric-loader}`, `libraries/org/quiltmc/quilt-loader`, `libraries/net/minecraftforge/forge`, `libraries/net/neoforged/neoforge`. Une valeur qui n'a pas la forme d'une version est ignorée.
2. Version : réponse absente, vide ou « LATEST » et version lue : la version lue est appliquée. Version choisie différente de la version lue : refus `PACK_VERSION_MISMATCH`. Version non lue et réponse « LATEST » : refus `PACK_VERSION_UNREADABLE` (« choisissez la version »). Version non lue et version choisie : acceptée.
3. Moteur lu et variante choisie différente : refus `PACK_LOADER_MISMATCH`.
4. Aucun refus ne retire le dépôt : le compte corrige et relance. Un pack Modrinth n'est pas concerné (l'image lit le pack).
5. Une modification faite après l'installation (HRT-87) n'est pas encore contrôlée contre le pack rangé.

## Application (code)
- `crates/hearth-agent/src/domain/mods/game.rs::detect`
- `crates/hearth-agent/src/domain/mods/pack.rs::{PackReport, PackRefusal}`
- `crates/hearth-agent/src/application/fiche_install.rs::apply_pack_game`

## Vérification
- `crates/hearth-agent/src/domain/mods/game.rs` (tests)
- `crates/hearth-agent/tests/minecraft_install_https.rs::a_fabric_1_20_1_pack_installs_with_the_game_version_it_says_not_latest`
- `crates/hearth-agent/tests/minecraft_install_https.rs::a_version_that_contradicts_the_pack_or_an_unreadable_one_left_on_latest_is_refused_and_the_pack_stays`

## Cas limites
- Le pack est hostile : seuls des fichiers de description de 1 Mio au plus sont lus, jamais recopiés dans une réponse.

## Règles liées
- BR-MOD-007, BR-MOD-008

## Historique
- 2026-11-16 : création (HRT-225, FIX-01M7GSNNTE0GP22STY1VRJ56YG).
