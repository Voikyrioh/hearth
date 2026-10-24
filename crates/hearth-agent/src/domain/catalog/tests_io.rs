//! Enregistrer dans le fichier du jeu : rien d'inconnu ne se perd, un écart demande un choix, un texte collé
//! s'analyse sans rien écrire (HRT-108, BR-FICHE-046 à 048). Sur la fiche Terraria embarquée.

use std::collections::BTreeMap;

use super::embedded::EMBEDDED;
use super::formats::{ConfigFormat, format_for};
use super::loading::model::Fiche;
use super::settings_io::{
    DriftChoice, ImportStatus, SaveFailure, SavePlan, analyze_import, plan_save,
};
use super::settings_state::{
    Change, Context, Effect, InstallContext, Phase, Reason, SettingsState, Slot, initial_state,
    resolve_install,
};
use crate::domain::secret::Secret;

fn terraria() -> Fiche {
    EMBEDDED
        .iter()
        .find(|e| e.dir == "terraria")
        .expect("terraria")
        .load()
        .expect("chargée")
}

fn format_of(fiche: &Fiche) -> Box<dyn ConfigFormat> {
    format_for(fiche.channel("config").expect("canal")).expect("format")
}

fn installed() -> Context<'static> {
    Context {
        phase: Phase::Installed,
        variant: None,
        game_version: None,
        world_imported: false,
    }
}

fn state(fiche: &Fiche, given: &[(&str, &str)]) -> SettingsState {
    let ports = BTreeMap::new();
    let generated: BTreeMap<String, Secret> = BTreeMap::new();
    let ctx = InstallContext {
        variant: None,
        world_imported: false,
        host_ports: &ports,
        generated: &generated,
    };
    let answers = given
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let resolved = resolve_install(fiche, &ctx, &answers).expect("réponses valides");
    initial_state(fiche, &resolved)
}

fn set(id: &str, value: &str) -> (String, Change) {
    (id.to_owned(), Change::Set(value.to_owned()))
}

fn save(
    fiche: &Fiche,
    state: &SettingsState,
    file: Option<&[u8]>,
    changes: &[(String, Change)],
    choices: &[(&str, DriftChoice)],
) -> Result<SavePlan, SaveFailure> {
    let choices = choices.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
    plan_save(
        fiche,
        &installed(),
        state,
        "config",
        format_of(fiche).as_ref(),
        file,
        changes,
        &choices,
        &[],
    )
}

/// Le fichier d'un service installé : un commentaire, une ligne commentée, un réglage inventé, des fins de ligne
/// CRLF, puis une ligne par réglage de l'état (`overrides` : ce que le fichier dit en vrai, clé du jeu), et une
/// dernière ligne sans fin de ligne.
fn file_of(fiche: &Fiche, state: &SettingsState, overrides: &[(&str, &str)]) -> Vec<u8> {
    let crlf = "\r\n";
    let mut out = format!("# Réglages{crlf}mod_setting_inventé=42{crlf}#secure=1{crlf}");
    let mut written: Vec<String> = Vec::new();
    for (id, slot) in state.iter() {
        let Some(setting) = fiche.setting(id) else {
            continue;
        };
        let value = overrides
            .iter()
            .find(|(k, _)| *k == setting.key)
            .map(|(_, v)| (*v).to_owned())
            .or_else(|| match slot {
                Slot::Value(v) => Some(v.clone()),
                _ => None,
            });
        if let Some(value) = value {
            out.push_str(&format!("{}={value}{crlf}", setting.key));
            written.push(setting.key.clone());
        }
    }
    for (key, value) in overrides {
        if !written.iter().any(|w| w == key) {
            out.push_str(&format!("{key}={value}{crlf}"));
        }
    }
    out.push_str("last=x");
    out.into_bytes()
}

fn text(plan: &SavePlan) -> String {
    String::from_utf8(plan.file.as_ref().expect("fichier à écrire").to_vec()).expect("utf8")
}

#[test]
fn an_invented_setting_and_a_comment_survive_every_gesture_of_br_fiche_047() {
    let fiche = terraria();
    // L'état voulu colle au fichier, sauf `motd` que le fichier porte et que l'état ignore.
    let mut current = state(&fiche, &[("world_name", "Monde")]);
    let mut file = file_of(&fiche, &current, &[]);
    let gestures: Vec<Vec<(String, Change)>> = vec![
        // enregistrement
        vec![set("motd", "Nouveau")],
        // remise aux valeurs proposées
        vec![("motd".into(), Change::Reset)],
        // import (les mêmes changements que le formulaire, l'origine est dans l'acte)
        vec![set("max_players", "12"), set("language", "fr-FR")],
        // restauration : retour à « non écrit »
        vec![("language".into(), Change::Unset)],
    ];
    for gesture in gestures {
        let plan = save(&fiche, &current, Some(&file), &gesture, &[]).expect("enregistré");
        current = plan.state;
        if let Some(bytes) = plan.file {
            file = bytes.to_vec();
        }
    }
    let out = String::from_utf8(file).expect("utf8");
    for kept in [
        "# Réglages\r\n",
        "mod_setting_inventé=42\r\n",
        "#secure=1\r\n",
        "last=x",
    ] {
        assert!(out.contains(kept), "{kept:?} a disparu : {out:?}");
    }
    assert!(out.contains("maxplayers=12"));
    assert!(!out.contains("language="), "remis à non écrit");
    assert!(
        !out.contains("motd="),
        "la remise sans valeur proposée retire la ligne"
    );
}

#[test]
fn nothing_is_written_when_nothing_changed() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde"), ("motd", "Ancien")]);
    let file = file_of(&fiche, &current, &[]);
    let plan = save(&fiche, &current, Some(&file), &[], &[]).expect("rien à faire");
    assert!(
        plan.file.is_none(),
        "aucun changement demandé : aucune écriture"
    );
    assert_eq!(plan.effect, Effect::None);
    let same = save(&fiche, &current, Some(&file), &[set("motd", "Ancien")], &[])
        .expect("le fichier dit déjà « Ancien »");
    assert!(same.file.is_none(), "la ligne est déjà identique");
    // `world_name` est figé après l'installation.
    let frozen = save(
        &fiche,
        &current,
        Some(&file),
        &[set("world_name", "Autre")],
        &[],
    );
    assert!(matches!(
        frozen,
        Err(SaveFailure::Refused(ref r)) if r.reason == Reason::Frozen
    ));
}

#[test]
fn a_file_the_game_rewrote_asks_for_a_choice_per_setting_and_nothing_is_written_without_it() {
    let fiche = terraria();
    // Hearth croit « motd = Voulu », le fichier dit « Ancien ».
    let current = state(&fiche, &[("world_name", "Monde"), ("motd", "Voulu")]);
    let file = file_of(&fiche, &current, &[("motd", "Ancien")]);
    let refused = save(
        &fiche,
        &current,
        Some(&file),
        &[set("max_players", "9")],
        &[],
    );
    assert!(
        matches!(refused, Err(SaveFailure::DriftUnresolved(ref ids)) if ids == &["motd".to_owned()]),
        "{refused:?}"
    );
    // Garder le fichier : la description reprend « Ancien », la ligne n'est pas touchée.
    let kept = save(
        &fiche,
        &current,
        Some(&file),
        &[set("max_players", "9")],
        &[("motd", DriftChoice::KeepFile)],
    )
    .expect("choix fait");
    assert_eq!(kept.state.value("motd"), Some("Ancien"));
    assert_eq!(kept.adopted, vec!["motd".to_owned()]);
    assert!(text(&kept).contains("motd=Ancien\r\n"));
    assert!(text(&kept).contains("maxplayers=9"));
    // Remettre la mienne : le fichier reçoit « Voulu ».
    let mine = save(
        &fiche,
        &current,
        Some(&file),
        &[],
        &[("motd", DriftChoice::KeepMine)],
    )
    .expect("choix fait");
    assert!(text(&mine).contains("motd=Voulu\r\n"));
    assert_eq!(mine.state.value("motd"), Some("Voulu"));
    // Changer soi-même le réglage en écart vaut choix.
    let changed = save(&fiche, &current, Some(&file), &[set("motd", "Neuf")], &[])
        .expect("le changement vaut choix");
    assert!(text(&changed).contains("motd=Neuf\r\n"));
}

#[test]
fn an_unreadable_file_is_refused_whole_and_nothing_is_planned() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde")]);
    for bad in [&b"a=1\0b=2"[..], &[0xFF, 0xFE, b'a', 0][..], b"a=1\rb=2"] {
        let result = save(&fiche, &current, Some(bad), &[set("motd", "x")], &[]);
        assert!(
            matches!(result, Err(SaveFailure::Unreadable(_))),
            "{result:?}"
        );
    }
}

#[test]
fn a_refused_change_writes_nothing_and_names_the_setting_never_the_value() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde")]);
    let file = file_of(&fiche, &current, &[]);
    let result = save(
        &fiche,
        &current,
        Some(&file),
        &[set("max_players", "9999"), set("motd", "ok")],
        &[],
    );
    match result {
        Err(SaveFailure::Refused(refusal)) => {
            assert_eq!(refusal.id, "max_players");
            assert_eq!(refusal.reason, Reason::Bounds);
        }
        other => panic!("{other:?}"),
    }
    // Un réglage imposé ne se change pas.
    let imposed = save(&fiche, &current, Some(&file), &[set("upnp", "1")], &[]);
    assert!(matches!(
        imposed,
        Err(SaveFailure::Refused(ref r)) if r.reason == Reason::Imposed
    ));
}

#[test]
fn a_missing_file_is_created_with_what_is_set_and_a_secret_stays_out_of_every_debug_text() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde")]);
    let sentinel = "SENTINELLE-mot-de-passe-9f3a";
    let plan = save(
        &fiche,
        &current,
        None,
        &[set("password", sentinel), set("motd", "Salut")],
        &[],
    )
    .expect("créé");
    assert!(text(&plan).contains(&format!("password={sentinel}")));
    assert!(text(&plan).contains("motd=Salut"));
    assert_eq!(plan.state.slot("password"), Some(&Slot::Secret));
    assert_eq!(plan.secrets.len(), 1);
    let shown = format!("{plan:?}");
    assert!(!shown.contains(sentinel), "{shown}");
    let state_shown = format!("{:?}", plan.state);
    assert!(!state_shown.contains(sentinel), "{state_shown}");
}

#[test]
fn analysing_pasted_text_judges_each_setting_without_writing_and_counts_the_unknown() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde"), ("motd", "Ancien")]);
    let mut imposed = BTreeMap::new();
    imposed.insert("upnp".to_owned(), "0".to_owned());
    let pasted = "motd=Ancien\nmaxplayers=12\nmaxplayers=14\nlanguage=klingon\nupnp=1\npassword=secretissime\nworldname=Autre\nfoo_mod=1\n# commentaire\n";
    let analysis = analyze_import(
        &fiche,
        &installed(),
        &current,
        "config",
        format_of(&fiche).as_ref(),
        pasted.as_bytes(),
        &imposed,
    )
    .expect("lisible");
    let status = |id: &str| {
        analysis
            .lines
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.status.clone())
    };
    assert_eq!(status("motd"), Some(ImportStatus::Same));
    assert_eq!(
        status("max_players"),
        Some(ImportStatus::Take),
        "la dernière valeur est lue"
    );
    assert_eq!(
        status("language"),
        Some(ImportStatus::Rejected(Reason::Choice))
    );
    assert_eq!(status("upnp"), Some(ImportStatus::Imposed));
    assert_eq!(status("world_name"), Some(ImportStatus::Frozen));
    assert_eq!(status("password"), Some(ImportStatus::Take));
    assert!(
        analysis
            .lines
            .iter()
            .find(|l| l.id == "password")
            .expect("ligne")
            .secret
    );
    assert_eq!(analysis.unknown, 1);
    assert_eq!(analysis.duplicates, 1);
    assert_eq!(analysis.imposed_undone, vec!["upnp".to_owned()]);
    let shown = format!("{analysis:?}");
    assert!(!shown.contains("secretissime"), "{shown}");
    // Un texte illisible est refusé entier.
    assert!(
        analyze_import(
            &fiche,
            &installed(),
            &current,
            "config",
            format_of(&fiche).as_ref(),
            b"a=1\0",
            &imposed
        )
        .is_err()
    );
}

#[test]
fn a_file_value_the_setting_does_not_accept_is_never_adopted_as_the_wanted_value() {
    let fiche = terraria();
    let current = state(&fiche, &[("world_name", "Monde")]);
    let file = file_of(&fiche, &current, &[("maxplayers", "9999")]);
    // « Garder la valeur du fichier » : refusé, c'est une valeur que le réglage n'accepte pas.
    let kept = save(
        &fiche,
        &current,
        Some(&file),
        &[],
        &[("max_players", DriftChoice::KeepFile)],
    );
    assert!(matches!(
        kept,
        Err(SaveFailure::Refused(ref r)) if r.id == "max_players" && r.reason == Reason::Bounds
    ));
    // « Remettre la mienne » reste possible : la valeur voulue réécrit la ligne.
    let mine = save(
        &fiche,
        &current,
        Some(&file),
        &[],
        &[("max_players", DriftChoice::KeepMine)],
    )
    .expect("choix fait");
    assert!(text(&mine).contains("maxplayers=8"));
}

#[test]
fn a_name_that_looks_like_a_secret_is_recognised_whatever_the_case() {
    use super::settings_state::looks_secret;
    for name in [
        "password",
        "ADMIN_PASS",
        "api_token",
        "Secret_x",
        "steam_key",
    ] {
        assert!(looks_secret(name), "{name}");
    }
    for name in ["motd", "maxplayers", "banlist", "worldname"] {
        assert!(!looks_secret(name), "{name}");
    }
}
