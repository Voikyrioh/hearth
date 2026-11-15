//! Le bloc `[mods]` de la fiche (HRT-124) et l'invariant « aucune voie CurseForge » (BR-MOD-017) : fiches piégées
//! dérivées d'UNE modification de la fiche riche, refusées avec LE bon motif.

use super::embedded::load_all;
use super::loading::error::{FicheRefusee, Motif};
use super::loading::invariants::{is_curseforge_name, is_curseforge_value};
use super::loading::model::{Fiche, ValueSpec};
use super::loading::schema::fiche::{ModWay, PackGenre};
use super::test_support::*;

const PACKS_FOLDER: &str = "[[folder]]\nid = \"packs\"\nrole = \"packs\"\ntarget = \"/packs\"\nread_only = true\nbackup = false\n\n";
const MODS_BLOCK: &str = "[mods]\nways = [\"drop_files\", \"drop_pack\"]\n\n[mods.pack]\nfolder = \"packs\"\n\n[[mods.pack.genre]]\nkind = \"server_pack\"\nenv = \"GENERIC_PACK\"\nvariants = [\"java\"]\n\n[[mods.pack.genre]]\nkind = \"modrinth_pack\"\nenv = \"MODRINTH_MODPACK\"\nvariants = [\"bedrock\"]\n";

/// La fiche riche, plus un dossier de packs avant son dossier de mods, plus `mods` en fin de fichier.
fn rich_with(mods: &str) -> String {
    let with_folder = replace(
        RICH_FICHE,
        "[[folder]]\nid = \"mods\"",
        &format!("{PACKS_FOLDER}[[folder]]\nid = \"mods\""),
    );
    format!("{with_folder}\n{mods}\n")
}

fn load_rich(fiche: &str) -> Result<Fiche, FicheRefusee> {
    load_texts(fiche, RICH_SETTINGS)
}

fn refused(fiche: &str) -> Motif {
    match load_rich(fiche) {
        Ok(_) => panic!("la fiche piégée a été acceptée"),
        Err(refusal) => refusal.motif,
    }
}

#[test]
fn a_fiche_without_a_mods_block_loads_as_before() {
    assert!(rich().file.mods.is_none());
}

#[test]
fn a_fiche_declares_its_ways_and_the_genres_of_pack_it_takes() {
    let fiche = load_rich(&rich_with(MODS_BLOCK)).expect("fiche");
    let mods = fiche.file.mods.expect("mods");
    assert_eq!(mods.ways, vec![ModWay::DropFiles, ModWay::DropPack]);
    let pack = mods.pack.expect("pack");
    assert_eq!(pack.folder, "packs");
    assert_eq!(pack.genre[0].kind, PackGenre::ServerPack);
    assert_eq!(pack.genre[1].env, "MODRINTH_MODPACK");
}

#[test]
fn a_way_a_genre_or_a_field_outside_the_closed_lists_is_refused() {
    let ok = rich_with(MODS_BLOCK);
    for (from, to) in [
        (
            "ways = [\"drop_files\", \"drop_pack\"]",
            "ways = [\"drop_files\", \"fetch_from_curseforge\"]",
        ),
        ("kind = \"server_pack\"", "kind = \"curseforge_export\""),
        (
            "folder = \"packs\"\n",
            "folder = \"packs\"\napi_key = \"x\"\n",
        ),
    ] {
        assert!(load_rich(&replace(&ok, from, to)).is_err(), "{to}");
    }
}

#[test]
fn the_mods_block_must_be_coherent() {
    let ok = rich_with(MODS_BLOCK);
    let both = "[\"drop_files\", \"drop_pack\"]";
    assert!(matches!(
        refused(&replace(&ok, both, "[\"drop_pack\", \"drop_pack\"]")),
        Motif::ModsIncoherent { .. }
    ));
    assert!(matches!(
        refused(&replace(&ok, both, "[]")),
        Motif::ModsIncoherent { .. }
    ));
    assert!(matches!(
        refused(&rich_with("[mods]\nways = [\"drop_pack\"]")),
        Motif::ModsIncoherent { .. }
    ));
    assert!(matches!(
        refused(&replace(&ok, both, "[\"drop_files\"]")),
        Motif::ModsIncoherent { .. }
    ));
    assert!(matches!(
        refused(&replace(&ok, "role = \"mods\"", "role = \"data\"")),
        Motif::ModsIncoherent { .. }
    ));
    for (from, to) in [
        (
            "target = \"/packs\"\nread_only = true",
            "target = \"/packs\"\nread_only = false",
        ),
        (
            "read_only = true\nbackup = false",
            "read_only = true\nbackup = true",
        ),
        ("role = \"packs\"", "role = \"data\""),
    ] {
        assert!(
            matches!(
                refused(&replace(&ok, from, to)),
                Motif::ModsIncoherent { .. }
            ),
            "{to}"
        );
    }
    assert!(matches!(
        refused(&replace(&ok, "folder = \"packs\"", "folder = \"nope\"")),
        Motif::DossierInconnu { .. }
    ));
    assert!(matches!(
        refused(&replace(
            &ok,
            "variants = [\"java\"]",
            "variants = [\"quilt\"]"
        )),
        Motif::ReferenceInconnue { .. }
    ));
    assert!(matches!(
        refused(&replace(
            &ok,
            "env = \"GENERIC_PACK\"",
            "env = \"generic pack\""
        )),
        Motif::IdentifiantInvalide { .. }
    ));
    assert!(matches!(
        refused(&replace(&ok, "variants = [\"java\"]", "variants = []")),
        Motif::ListeDeVariantesVide
    ));
}

#[test]
fn br_mod_017_no_cf_variable_and_no_auto_curseforge_anywhere() {
    let at = |ou: &str| Motif::CurseForgeInterdit { ou: ou.into() };
    let ok = rich_with(MODS_BLOCK);
    let puid = "[[env]]\nname = \"PUID\"";
    let with_env = |name: &str, value: &str| {
        replace(
            &ok,
            puid,
            &format!("[[env]]\nname = \"{name}\"\nvalue = \"{value}\"\n\n{puid}"),
        )
    };
    assert_eq!(refused(&with_env("CF_API_KEY", "x")), at("variable fixe"));
    assert_eq!(
        refused(&with_env("TYPE", " auto_curseforge ")),
        at("variable fixe")
    );
    assert_eq!(
        refused(&replace(&ok, "name = \"EDITION\"", "name = \"CF_SLUG\"")),
        at("variable d'une variante")
    );
    assert_eq!(
        refused(&replace(
            &ok,
            "env = \"MODRINTH_MODPACK\"",
            "env = \"CF_PAGE_URL\""
        )),
        at("variable d'un genre de pack")
    );
    let setting = |key: &str, extra: &str| {
        format!(
            "[[setting]]\nid = \"cf_setting\"\nkey = \"{key}\"\nchannel = \"env\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Réglage\"\nhelp = \"Aide.\"\ntype = \"text\"\ncharset = \"line\"\nmax_len = 40\n{extra}change = \"anytime\"\nsource = \"partial\"\n"
        )
    };
    for (key, extra) in [
        ("CF_SLUG", ""),
        ("TYPE", "proposed = \"AUTO_CURSEFORGE\"\n"),
        ("TYPE", "game_default = \"AUTO_CURSEFORGE\"\n"),
        (
            "TYPE",
            "imposed = { value = \"AUTO_CURSEFORGE\", why = \"Non.\" }\n",
        ),
    ] {
        let settings = format!("{RICH_SETTINGS}\n{}", setting(key, extra));
        match load_texts(RICH_FICHE, &settings) {
            Err(refusal) => assert_eq!(refusal.motif, at("réglage"), "{key} {extra}"),
            Ok(_) => panic!("réglage CurseForge accepté : {key} {extra}"),
        }
    }
}

#[test]
fn no_embedded_fiche_carries_a_curseforge_way() {
    // Garde sur les fiches réelles (BR-MOD-017) : aucune variable CF_*, aucun AUTO_CURSEFORGE, nulle part.
    let (fiches, refusees) = load_all();
    assert!(refusees.is_empty(), "fiche refusée");
    for fiche in fiches {
        let file = &fiche.file;
        let mut named: Vec<(String, Option<String>)> = Vec::new();
        named.extend(file.env.iter().map(|e| (e.name.clone(), e.value.clone())));
        for variant in &file.variant {
            named.extend(
                variant
                    .env
                    .iter()
                    .map(|e| (e.name.clone(), Some(e.value.clone()))),
            );
        }
        for setting in &fiche.settings {
            named.push((setting.key.clone(), setting.proposed.clone()));
            named.push((setting.key.clone(), setting.game_default.clone()));
            if let ValueSpec::Choice { choices } = &setting.spec {
                named.extend(
                    choices
                        .iter()
                        .map(|c| (String::new(), Some(c.value.clone()))),
                );
            }
        }
        for (name, value) in named {
            assert!(!is_curseforge_name(&name), "{}: {name}", file.id);
            assert!(
                !value.as_deref().is_some_and(is_curseforge_value),
                "{}: {name}",
                file.id
            );
        }
    }
}

#[test]
fn the_curseforge_ban_is_not_a_prefix_game() {
    let at = |ou: &str| Motif::CurseForgeInterdit { ou: ou.into() };
    let ok = rich_with(MODS_BLOCK);
    let puid = "[[env]]\nname = \"PUID\"";
    let with_env = |name: &str, value: &str| {
        replace(
            &ok,
            puid,
            &format!("[[env]]\nname = \"{name}\"\nvalue = \"{value}\"\n\n{puid}"),
        )
    };
    // Le nom ou la valeur contient CURSEFORGE, où que ce soit, quelle que soit la casse.
    assert_eq!(
        refused(&with_env("CURSEFORGE_FILES", "x")),
        at("variable fixe")
    );
    assert_eq!(
        refused(&with_env("TYPE", "curseforge")),
        at("variable fixe")
    );
    assert_eq!(
        refused(&with_env("TYPE", "Auto_CurseForge")),
        at("variable fixe")
    );
}

#[test]
fn the_server_type_is_never_a_free_text_setting() {
    // Un réglage libre sur `TYPE` laisserait taper AUTO_CURSEFORGE à l'installation.
    let free = "[[setting]]\nid = \"server_type\"\nkey = \"TYPE\"\nchannel = \"env\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Type\"\nhelp = \"Aide.\"\ntype = \"text\"\ncharset = \"line\"\nmax_len = 20\nchange = \"anytime\"\nsource = \"partial\"\n";
    match load_texts(RICH_FICHE, &format!("{RICH_SETTINGS}\n{free}")) {
        Err(refusal) => assert_eq!(
            refusal.motif,
            Motif::CurseForgeInterdit {
                ou: "réglage".into()
            }
        ),
        Ok(_) => panic!("un type de serveur en texte libre a été accepté"),
    }
    // Un choix fermé sans CurseForge reste permis.
    let closed = free
        .replace("type = \"text\"\ncharset = \"line\"\nmax_len = 20\n", "type = \"choice\"\nchoices = [ { value = \"PAPER\", label = \"Paper\" }, { value = \"FABRIC\", label = \"Fabric\" } ]\n");
    assert!(load_texts(RICH_FICHE, &format!("{RICH_SETTINGS}\n{closed}")).is_ok());
}

#[test]
fn a_packs_folder_is_read_only_and_never_saved_with_or_without_a_mods_block() {
    let writable = replace(
        RICH_FICHE,
        "[[folder]]\nid = \"mods\"",
        "[[folder]]\nid = \"packs\"\nrole = \"packs\"\ntarget = \"/packs\"\nread_only = false\nbackup = false\n\n[[folder]]\nid = \"mods\"",
    );
    assert!(matches!(refused(&writable), Motif::ModsIncoherent { .. }));
    let saved = writable.replace(
        "read_only = false\nbackup = false\n\n[[folder]]\nid = \"mods\"",
        "read_only = true\nbackup = true\n\n[[folder]]\nid = \"mods\"",
    );
    assert!(matches!(refused(&saved), Motif::ModsIncoherent { .. }));
}
