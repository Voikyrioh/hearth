---
id: BR-MOD-022
domaine: MOD
titre: Un jar se lit comme une archive hostile : au plus cinq fichiers d'identité, rien d'exécuté
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-022 : Un jar se lit comme une archive hostile : au plus cinq fichiers d'identité, rien d'exécuté

## Règle
Le contrôle juge le jar ENTIER par la lecture d'archive hostile (bornes `jar_limits()` : 256 Mio le fichier, 1 Gio déballé, 65 536 entrées, ADR-0055), puis lit au plus `fabric.mod.json`, `quilt.mod.json`, `META-INF/neoforge.mods.toml`, `META-INF/mods.toml` et `META-INF/MANIFEST.MF`, 1 Mio chacun. Aucune archive contenue n'est ouverte, aucune classe chargée. Toute valeur lue passe une forme (identifiant `[a-z][a-z0-9_.-]{0,63}`, version `[A-Za-z0-9.+_-]{1,64}`, nom de 1 à 64 caractères sans contrôle ni caractère bidirectionnel) ; sinon elle est ignorée, jamais recopiée. `${file.jarVersion}` se résout par le manifeste, sinon la version est absente. Le fichier est posé sous le nom `{identifiant}-{version}.jar`, jamais sous le nom envoyé. (Parties « zone d'attente », « rejugement » et « refus au dépôt » : HRT-257, HRT-258.)

## Application (code)
- `crates/hearth-agent/src/domain/mods/jar.rs::{identify, IDENTITY_FILES, ModId::parse, ModVersion::parse}`
- `crates/hearth-agent/src/domain/mods/listing.rs::{placed_name, is_safe_file_name}`
- `crates/hearth-agent/src/application/pack_intake.rs::jar_limits`

## Vérification
- `domain::mods::jar::tests::{hostile_files_give_no_identity_and_never_panic, a_forge_jar_resolves_its_version_from_the_manifest_or_leaves_it_absent}`
- `domain::mods::listing::tests::*`
- `application::pack_intake::tests::the_jar_limits_are_256_mib_one_gib_and_65536_entries`

## Cas limites
- JSON très profond ou TOML à clés en double : aucune identité, aucune panique.
- Fichier d'identité de 1 Mio + 1 : ignoré.

## Règles liées
- BR-MOD-012, BR-MOD-021, BR-ARCH-001 à 011

## Historique
- 2026-11-18 : création (lot 0.2.2, mods d'un serveur installé).
