//! État des réglages, effet déduit du canal, écart, réglages imposés, plan d'arrêt : sur la fiche Terraria
//! embarquée et sur la fiche riche.

use std::collections::BTreeMap;

use super::embedded::EMBEDDED;
use super::formats::format_for;
use super::loading::model::Fiche;
use super::settings_state::{
    Change, Context, Effect, InstallContext, Phase, Reason, Slot, VersionStatus, apply_changes,
    display_value, drift, effect_of, essential_questions, imposed_broken, initial_state,
    resolve_install, version_status,
};
use super::stop_plan::{
    DecisionReason, PlannedStep, Signal, StopPlan, game_version_reader, ready_sign, save_plan,
    stop_plan,
};
use super::test_support::*;
use crate::domain::secret::Secret;

fn terraria() -> Fiche {
    EMBEDDED
        .iter()
        .find(|e| e.dir == "terraria")
        .expect("terraria")
        .load()
        .expect("chargée")
}

fn no_ports() -> BTreeMap<String, u16> {
    BTreeMap::new()
}

fn no_secrets() -> BTreeMap<String, Secret> {
    BTreeMap::new()
}

fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn install_state(
    fiche: &Fiche,
    given: &[(&str, &str)],
    world_imported: bool,
) -> super::settings_state::SettingsState {
    let ports = no_ports();
    let generated = no_secrets();
    let ctx = InstallContext {
        variant: None,
        world_imported,
        host_ports: &ports,
        generated: &generated,
    };
    let resolved = resolve_install(fiche, &ctx, &answers(given)).expect("réponses valides");
    initial_state(fiche, &resolved)
}

fn installed(variant: Option<&'static str>) -> Context<'static> {
    Context {
        phase: Phase::Installed,
        variant,
        game_version: None,
        world_imported: false,
    }
}

// ---- Trois états : une valeur, la valeur proposée, non écrit ------------------------------------------------------------------

#[test]
fn not_written_is_a_value_distinct_from_the_default_of_the_game() {
    let fiche = terraria();
    let state = install_state(&fiche, &[], false);
    // Proposé : écrit.
    assert_eq!(state.value("world_name"), Some("Monde"));
    assert_eq!(state.value("max_players"), Some("8"));
    assert_eq!(state.value("world_size"), Some("3"));
    assert_eq!(state.value("difficulty"), Some("2"));
    assert_eq!(state.value("secure"), Some("1"));
    // Rien de proposé : non écrit, même quand l'inventaire connaît le défaut du jeu.
    assert_eq!(
        fiche
            .setting("language")
            .expect("language")
            .game_default
            .as_deref(),
        Some("en-US")
    );
    assert_eq!(
        state.slot("language"),
        Some(&Slot::Unwritten),
        "« non écrit » n'est pas « valeur par défaut »"
    );
    assert_eq!(state.slot("npc_stream"), Some(&Slot::Unwritten));
    assert_eq!(state.slot("password"), Some(&Slot::Unwritten));
    assert_eq!(state.slot("journey_godmode"), Some(&Slot::Unwritten));
    assert_eq!(state.slot("announcement_box_off"), Some(&Slot::Unwritten));
    // Imposé : écrit avec la valeur calculée.
    assert_eq!(
        state.value("world_file"),
        Some("/root/.local/share/Terraria/Worlds/Monde.wld")
    );
    assert_eq!(
        state.value("world_path"),
        Some("/root/.local/share/Terraria/Worlds/")
    );
    assert_eq!(state.value("port"), Some("7777"));
    assert_eq!(state.value("upnp"), Some("0"));
}

#[test]
fn answers_replace_the_proposed_values_and_a_derived_value_follows() {
    let fiche = terraria();
    let state = install_state(
        &fiche,
        &[
            ("world_name", "Foret"),
            ("motd", "Salut"),
            ("password", "s3cret"),
        ],
        false,
    );
    assert_eq!(state.value("world_name"), Some("Foret"));
    assert_eq!(
        state.value("world_file"),
        Some("/root/.local/share/Terraria/Worlds/Foret.wld")
    );
    assert_eq!(state.value("motd"), Some("Salut"));
    assert_eq!(
        state.slot("password"),
        Some(&Slot::Secret),
        "un secret n'est jamais dans l'état"
    );
    assert!(!format!("{state:?}").contains("s3cret"));
}

#[test]
fn a_refused_value_names_the_reason_and_at_most_one_character_never_the_value() {
    let fiche = terraria();
    let ports = no_ports();
    let generated = no_secrets();
    let ctx = InstallContext {
        variant: None,
        world_imported: false,
        host_ports: &ports,
        generated: &generated,
    };
    let try_answer = |id: &str, value: &str| {
        resolve_install(&fiche, &ctx, &answers(&[(id, value)])).expect_err("refusée")
    };
    let long_motd = "m".repeat(101);
    let cases: Vec<(&str, &str, Reason, Option<char>)> = vec![
        ("max_players", "999", Reason::Bounds, None),
        ("max_players", "abc", Reason::Bounds, None),
        ("max_players", "08", Reason::Bounds, None),
        ("world_name", "Mon monde", Reason::Charset, Some(' ')),
        ("world_name", "a/b", Reason::Charset, Some('/')),
        ("world_name", "a\u{e9}b", Reason::Charset, Some('\u{e9}')),
        ("motd", "a\nb=c", Reason::Charset, Some('\n')),
        ("motd", "a\rb", Reason::Charset, Some('\r')),
        // Une seule règle pour la validation et l'écriture : l'espace de bord est refusé dès la validation.
        ("motd", "Bonjour ", Reason::Charset, Some(' ')),
        ("motd", " Bonjour", Reason::Charset, Some(' ')),
        // Un secret ne cite jamais un caractère de sa valeur.
        ("password", "secret ", Reason::Charset, None),
        ("motd", long_motd.as_str(), Reason::Length, None),
        ("password", "abc\nexit", Reason::Charset, None),
        ("language", "xx-XX", Reason::Choice, None),
        ("secure", "yes", Reason::Choice, None),
        ("announcement_box_range", "-2", Reason::Bounds, None),
        ("world_file", "x.wld", Reason::Imposed, None),
        ("port", "7778", Reason::Imposed, None),
        ("upnp", "1", Reason::Imposed, None),
        ("nope", "1", Reason::UnknownId, None),
    ];
    for (id, value, reason, character) in cases {
        let refusal = try_answer(id, value);
        assert_eq!(
            (refusal.reason, refusal.character),
            (reason, character),
            "{id}"
        );
        assert_eq!(refusal.id, id);
        let shown = format!("{refusal:?}");
        if value.len() > 3 {
            assert!(
                !shown.contains(value),
                "{id} : le refus cite la valeur : {shown}"
            );
        }
    }
}

#[test]
fn a_world_taken_over_carries_its_frozen_settings() {
    let fiche = terraria();
    let state = install_state(&fiche, &[], true);
    for id in ["world_size", "difficulty", "seed"] {
        assert_eq!(
            state.slot(id),
            Some(&Slot::Unwritten),
            "{id} : porté par le monde repris"
        );
    }
    assert_eq!(state.value("world_name"), Some("Monde"));
    let ports = no_ports();
    let generated = no_secrets();
    let ctx = InstallContext {
        variant: None,
        world_imported: true,
        host_ports: &ports,
        generated: &generated,
    };
    let refusal =
        resolve_install(&fiche, &ctx, &answers(&[("difficulty", "1")])).expect_err("refusée");
    assert_eq!(refusal.reason, Reason::CarriedByWorld);
}

// ---- Changements, effet déduit du canal -----------------------------------------------------------------------------------------

#[test]
fn the_effect_of_a_change_is_deduced_from_the_channel_never_declared() {
    let fiche = terraria();
    let state = install_state(&fiche, &[], false);
    let effect = |id: &str| effect_of(&fiche, fiche.setting(id).expect(id));
    assert_eq!(effect("motd"), Effect::Restart, "canal file : redémarrage");
    assert_eq!(
        effect("announcement_box_range"),
        Effect::Recreate,
        "canal args : recréation"
    );
    let ctx = installed(None);
    let applied = apply_changes(
        &fiche,
        &ctx,
        &state,
        &[("motd".into(), Change::Set("Salut".into()))],
    )
    .expect("permis");
    assert_eq!(applied.effect, Effect::Restart);
    assert_eq!(applied.changed, vec!["motd".to_owned()]);
    assert_eq!(applied.edits.len(), 1);
    assert_eq!(
        (
            applied.edits[0].channel.as_str(),
            applied.edits[0].key.as_str()
        ),
        ("config", "motd")
    );
    assert_eq!(
        applied.edits[0].value.as_ref().map(Secret::expose),
        Some("Salut")
    );
    // Un argument : recréation, et aucune retouche de fichier.
    let applied = apply_changes(
        &fiche,
        &ctx,
        &state,
        &[("announcement_box_range".into(), Change::Set("100".into()))],
    )
    .expect("permis");
    assert_eq!(applied.effect, Effect::Recreate);
    assert!(applied.edits.is_empty());
    // Les deux ensemble : le plus fort l'emporte.
    let applied = apply_changes(
        &fiche,
        &ctx,
        &state,
        &[
            ("motd".into(), Change::Set("x".into())),
            ("announcement_box_range".into(), Change::Set("5".into())),
        ],
    )
    .expect("permis");
    assert_eq!(applied.effect, Effect::Recreate);
    // Remettre la même valeur ne change rien.
    let applied = apply_changes(
        &fiche,
        &ctx,
        &state,
        &[("max_players".into(), Change::Set("8".into()))],
    )
    .expect("permis");
    assert!(applied.changed.is_empty());
    assert_eq!(applied.effect, Effect::None);
    assert!(applied.edits.is_empty());
}

#[test]
fn a_frozen_an_imposed_a_retired_and_an_unknown_setting_cannot_be_changed() {
    let fiche = terraria();
    let state = install_state(&fiche, &[], false);
    let ctx = installed(None);
    let refuse = |id: &str, change: Change| {
        apply_changes(&fiche, &ctx, &state, &[(id.to_owned(), change)]).expect_err("refusé")
    };
    assert_eq!(
        refuse("world_name", Change::Set("Autre".into())).reason,
        Reason::Frozen
    );
    assert_eq!(refuse("difficulty", Change::Reset).reason, Reason::Frozen);
    assert_eq!(
        refuse("port", Change::Set("7778".into())).reason,
        Reason::Imposed
    );
    assert_eq!(refuse("upnp", Change::Unset).reason, Reason::Imposed);
    assert_eq!(refuse("nope", Change::Unset).reason, Reason::UnknownId);
    // À l'installation, un réglage figé à la création du monde se règle.
    let install = Context {
        phase: Phase::Install,
        variant: None,
        game_version: None,
        world_imported: false,
    };
    apply_changes(
        &fiche,
        &install,
        &state,
        &[("world_name".into(), Change::Set("Autre".into()))],
    )
    .expect("permis à l'installation");
    // Rien n'est écrit si un seul changement d'un lot est refusé.
    let err = apply_changes(
        &fiche,
        &ctx,
        &state,
        &[
            ("motd".into(), Change::Set("ok".into())),
            ("port".into(), Change::Set("1".into())),
        ],
    )
    .expect_err("tout ou rien");
    assert_eq!(err.reason, Reason::Imposed);
}

#[test]
fn reset_puts_the_proposed_value_back_or_removes_the_line_when_nothing_is_proposed() {
    let fiche = terraria();
    let state = install_state(&fiche, &[("motd", "Salut")], false);
    let ctx = installed(None);
    let changes = vec![
        ("secure".to_owned(), Change::Set("0".into())),
        ("motd".to_owned(), Change::Reset),
    ];
    let applied = apply_changes(&fiche, &ctx, &state, &changes).expect("permis");
    assert_eq!(applied.state.value("secure"), Some("0"));
    let again = apply_changes(
        &fiche,
        &ctx,
        &applied.state,
        &[("secure".into(), Change::Reset)],
    )
    .expect("permis");
    assert_eq!(
        again.state.value("secure"),
        Some("1"),
        "la valeur proposée revient"
    );
    assert_eq!(again.edits[0].value.as_ref().map(Secret::expose), Some("1"));
    // « motd » ne propose rien : la remise RETIRE la ligne, le jeu reprend son défaut.
    let motd = applied
        .edits
        .iter()
        .find(|e| e.key == "motd")
        .expect("retouche de motd");
    assert!(motd.value.is_none(), "la ligne est retirée");
    assert_eq!(applied.state.slot("motd"), Some(&Slot::Unwritten));
}

#[test]
fn a_secret_is_sealed_never_kept_in_the_state_or_in_a_debug_output() {
    let fiche = terraria();
    let state = install_state(&fiche, &[], false);
    let applied = apply_changes(
        &fiche,
        &installed(None),
        &state,
        &[("password".into(), Change::Set("hunter2-hunter2".into()))],
    )
    .expect("permis");
    assert_eq!(applied.state.slot("password"), Some(&Slot::Secret));
    assert_eq!(applied.secrets.len(), 1);
    assert_eq!(applied.secrets[0].0, "password");
    assert_eq!(applied.secrets[0].1.expose(), "hunter2-hunter2");
    assert_eq!(
        applied.edits[0].value.as_ref().map(Secret::expose),
        Some("hunter2-hunter2")
    );
    assert!(!format!("{applied:?}").contains("hunter2"));
}

#[test]
fn cross_checks_variants_retired_settings_and_game_versions_are_enforced() {
    let fiche = rich();
    let ports = no_ports();
    let mut generated = BTreeMap::new();
    generated.insert("rcon".to_owned(), Secret::from("a".repeat(32)));
    let ctx = InstallContext {
        variant: Some("java"),
        world_imported: false,
        host_ports: &ports,
        generated: &generated,
    };
    let resolved = resolve_install(&fiche, &ctx, &answers(&[])).expect("valide");
    let state = initial_state(&fiche, &resolved);
    let install = Context {
        phase: Phase::Install,
        variant: Some("java"),
        game_version: None,
        world_imported: false,
    };
    // [[check]] : hardcore exige la difficulté difficile.
    let err = apply_changes(
        &fiche,
        &install,
        &state,
        &[("hardcore".into(), Change::Set("TRUE".into()))],
    )
    .expect_err("contrôle croisé");
    assert_eq!(
        (err.id.as_str(), err.reason),
        ("difficulty", Reason::Check(0))
    );
    apply_changes(
        &fiche,
        &install,
        &state,
        &[
            ("hardcore".into(), Change::Set("TRUE".into())),
            ("difficulty".into(), Change::Set("hard".into())),
        ],
    )
    .expect("les deux ensemble respectent le contrôle");
    // Variante : un réglage d'une autre variante est refusé.
    assert_eq!(
        apply_changes(
            &fiche,
            &install,
            &state,
            &[("bedrock_only".into(), Change::Set("1".into()))]
        )
        .expect_err("refusé")
        .reason,
        Reason::Variant
    );
    let bedrock = Context {
        phase: Phase::Install,
        variant: Some("bedrock"),
        game_version: None,
        world_imported: false,
    };
    apply_changes(
        &fiche,
        &bedrock,
        &state,
        &[("bedrock_only".into(), Change::Set("1".into()))],
    )
    .expect("permis pour bedrock");
    // Réglage retiré.
    assert_eq!(
        apply_changes(
            &fiche,
            &install,
            &state,
            &[("old_mode".into(), Change::Unset)]
        )
        .expect_err("retiré")
        .reason,
        Reason::Retired
    );
    // Version du jeu : un réglage daté.
    let dated = replace(
        RICH_SETTINGS,
        "type = \"text\"\ncharset = \"line\"\nmax_len = 64",
        "type = \"text\"\ncharset = \"line\"\nmax_len = 64\nversions = { since = \"1.5\", until = \"2.0\" }",
    );
    let dated = load_texts(RICH_FICHE, &dated).expect("valide");
    let motd = dated.setting("motd").expect("motd");
    assert_eq!(version_status(motd, None), VersionStatus::Unknown);
    assert_eq!(version_status(motd, Some("1.4")), VersionStatus::NotYet);
    assert_eq!(version_status(motd, Some("1.5")), VersionStatus::Applies);
    assert_eq!(version_status(motd, Some("1.99")), VersionStatus::Applies);
    assert_eq!(version_status(motd, Some("2.0")), VersionStatus::Gone);
    assert_eq!(
        version_status(motd, Some("10.1")),
        VersionStatus::Gone,
        "comparaison numérique, pas textuelle"
    );
    let old = Context {
        phase: Phase::Install,
        variant: Some("java"),
        game_version: Some("2.1"),
        world_imported: false,
    };
    assert_eq!(
        apply_changes(
            &dated,
            &old,
            &state,
            &[("motd".into(), Change::Set("x".into()))]
        )
        .expect_err("n'existe plus")
        .reason,
        Reason::Version
    );
}

// ---- Écart avec le fichier du jeu, réglages imposés défaits ----------------------------------------------------------------------------

#[test]
fn drift_compares_the_state_to_the_file_and_lists_unknown_lines_without_touching_them() {
    let fiche = terraria();
    let state = install_state(&fiche, &[("motd", "Salut")], false);
    let file: Vec<(String, String)> = [
        ("worldname", "Monde"),
        ("maxplayers", "8"),
        ("autocreate", "3"),
        ("difficulty", "2"),
        ("world", "/root/.local/share/Terraria/Worlds/Monde.wld"),
        ("worldpath", "/root/.local/share/Terraria/Worlds/"),
        ("port", "7777"),
        ("upnp", "0"),
        ("secure", "0"),
        ("motd", "Salut"),
        ("custom_mod_setting", "42"),
        ("banlist", "bans.txt"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let (drifts, extras) = drift(&fiche, &state, "config", &file);
    assert_eq!(drifts.len(), 1, "{drifts:?}");
    assert_eq!(drifts[0].id, "secure");
    assert_eq!(
        (drifts[0].hearth.as_deref(), drifts[0].file.as_deref()),
        (Some("1"), Some("0"))
    );
    assert_eq!(
        extras,
        vec!["custom_mod_setting".to_owned(), "banlist".to_owned()],
        "le fichier gagne : ces lignes ne sont jamais perdues"
    );
    // Une ligne que Hearth ne veut pas écrite mais que le fichier porte est aussi un écart.
    let mut with_language = file.clone();
    with_language.push(("language".into(), "fr-FR".into()));
    let (drifts, _) = drift(&fiche, &state, "config", &with_language);
    assert!(
        drifts.iter().any(|d| d.id == "language"
            && d.hearth.is_none()
            && d.file.as_deref() == Some("fr-FR"))
    );
}

#[test]
fn an_imposed_setting_undone_by_hand_is_reported_never_rewritten() {
    let fiche = terraria();
    let ports = no_ports();
    let generated = no_secrets();
    let ctx = InstallContext {
        variant: None,
        world_imported: false,
        host_ports: &ports,
        generated: &generated,
    };
    let resolved = resolve_install(&fiche, &ctx, &answers(&[])).expect("valide");
    let good: Vec<(String, String)> = [
        ("world", "/root/.local/share/Terraria/Worlds/Monde.wld"),
        ("worldpath", "/root/.local/share/Terraria/Worlds/"),
        ("port", "7777"),
        ("upnp", "0"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let mut channels = BTreeMap::new();
    channels.insert("config".to_owned(), good.clone());
    assert!(imposed_broken(&fiche, &resolved, &channels).is_empty());
    let mut broken = good.clone();
    broken[3].1 = "1".into();
    broken.remove(1);
    channels.insert("config".to_owned(), broken);
    assert_eq!(
        imposed_broken(&fiche, &resolved, &channels),
        vec!["world_path".to_owned(), "upnp".to_owned()]
    );
}

#[test]
fn the_questions_of_an_installation_are_eight_at_most_and_a_secret_shows_no_value() {
    let fiche = terraria();
    let questions = essential_questions(&fiche);
    assert_eq!(questions.len(), 6);
    assert!(questions.len() <= 8);
    let password = fiche.setting("password").expect("password");
    assert_eq!(display_value(password, "secret"), None);
    let difficulty = fiche.setting("difficulty").expect("difficulty");
    assert_eq!(display_value(difficulty, "2"), Some("Maître"));
}

// ---- Edits appliqués à un fichier : rien d'inconnu ne se perd -------------------------------------------------------------------------------

#[test]
fn applying_edits_to_the_game_file_keeps_every_unknown_line_through_every_gesture() {
    let fiche = terraria();
    let channel = fiche.channel("config").expect("canal");
    let format = format_for(channel).expect("format");
    let original: &[u8] = "# Réglages\r\nunknown_mod_setting=42\r\n#secure=1\r\nmotd=Ancien\r\nbanlist=bans.txt\r\nlast=x".as_bytes();
    let mut doc = format.parse(original).expect("lisible");
    let state = install_state(&fiche, &[], false);
    let ctx = installed(None);
    let gestures: Vec<Vec<(String, Change)>> = vec![
        vec![("motd".into(), Change::Set("Nouveau".into()))],
        vec![("motd".into(), Change::Reset)],
        vec![
            ("secure".into(), Change::Set("0".into())),
            ("max_players".into(), Change::Set("12".into())),
        ],
        vec![("secure".into(), Change::Reset)],
        vec![("language".into(), Change::Set("fr-FR".into()))],
        vec![("language".into(), Change::Unset)],
        vec![("password".into(), Change::Set("pw-pw-pw".into()))],
    ];
    let mut current = state;
    for gesture in gestures {
        let applied = apply_changes(&fiche, &ctx, &current, &gesture).expect("permis");
        for edit in &applied.edits {
            match &edit.value {
                Some(value) => format
                    .set(&mut doc, &edit.key, value.expose())
                    .expect("écrit"),
                None => format.unset(&mut doc, &edit.key),
            }
        }
        current = applied.state;
    }
    let text = String::from_utf8(doc.render()).expect("utf8");
    for kept in [
        "# Réglages\r\n",
        "unknown_mod_setting=42\r\n",
        "#secure=1\r\n",
        "banlist=bans.txt\r\n",
        "last=x",
    ] {
        assert!(text.contains(kept), "{kept:?} a disparu : {text:?}");
    }
    assert!(
        !text.contains("motd="),
        "motd a été remis à « non écrit » : sa ligne est retirée"
    );
    assert!(!text.contains("language="));
    assert!(text.contains("password=pw-pw-pw"));
    assert!(text.contains("maxplayers=12\r\n"));
}

// ---- Plan d'arrêt ---------------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_sequence_that_was_not_tried_is_never_executed_it_asks_for_a_decision() {
    let terraria = terraria();
    assert_eq!(
        stop_plan(&terraria),
        StopPlan::NeedsDecision {
            reason: DecisionReason::UntestedSequence,
            grace_s: 10,
            signal: Signal::ImageDefault
        }
    );
    assert_eq!(
        save_plan(&terraria),
        None,
        "l'enregistrement à chaud non essayé n'est pas envoyé"
    );
    assert_eq!(ready_sign(&terraria), None);
    assert!(!stop_plan(&terraria).sends_orders());
    assert!(game_version_reader(&terraria).is_none());
}

#[test]
fn a_tested_sequence_ends_with_the_exit_order_then_the_engine_stop() {
    // Seul le mot « untested » change (« l'essai sur une copie ne change qu'un mot »).
    let text = include_str!("../../../catalog/terraria/fiche.toml")
        .replace("status = \"untested\"", "status = \"tested\"");
    assert_eq!(
        text.matches("status = \"tested\"").count(),
        3,
        "séquence d'arrêt, enregistrement, signe prêt"
    );
    let tried = load_texts(
        &text,
        include_str!("../../../catalog/terraria/settings.toml"),
    )
    .expect("valide");
    let plan = stop_plan(&tried);
    assert_eq!(
        plan,
        StopPlan::Sequence {
            steps: vec![
                PlannedStep::StdinLine {
                    line: "save".into(),
                    quits: false
                },
                PlannedStep::Wait { seconds: 15 },
                PlannedStep::StdinLine {
                    line: "exit".into(),
                    quits: true
                },
            ],
            grace_s: 10,
            signal: Signal::ImageDefault,
        }
    );
    assert!(plan.sends_orders());
    assert_eq!(plan.sequence_seconds(), 15);
    assert_eq!(
        plan.grace_s(),
        10,
        "l'arrêt du moteur suit l'ordre de sortie avec le délai de la fiche"
    );
    assert_eq!(
        save_plan(&tried),
        Some(vec![
            PlannedStep::StdinLine {
                line: "save".into(),
                quits: false
            },
            PlannedStep::Wait { seconds: 15 }
        ])
    );
    assert_eq!(ready_sign(&tried), Some("Server started"));
}

#[test]
fn every_plan_is_bounded_and_the_exit_order_always_comes_last() {
    for fiche in [rich(), base(), terraria()] {
        let plan = stop_plan(&fiche);
        assert!(plan.sequence_seconds() <= 20 * 60, "{}", fiche.id());
        assert!((10..=600).contains(&plan.grace_s()), "{}", fiche.id());
        if let StopPlan::Sequence { steps, .. } = &plan {
            assert!(steps.len() <= 6);
            let quits: Vec<usize> = steps
                .iter()
                .enumerate()
                .filter(|(_, s)| matches!(s, PlannedStep::StdinLine { quits: true, .. }))
                .map(|(i, _)| i)
                .collect();
            assert!(
                quits.is_empty() || quits == vec![steps.len() - 1],
                "l'ordre de sortie termine la séquence"
            );
        }
    }
    let rich_plan = stop_plan(&rich());
    assert_eq!(rich_plan.sequence_seconds(), 30);
    assert!(
        matches!(rich_plan, StopPlan::Sequence { ref steps, signal: Signal::Sigterm, .. } if steps.len() == 3)
    );
    assert!(matches!(&save_plan(&rich()), Some(steps) if steps.len() == 2));
    assert_eq!(ready_sign(&rich()), Some("Done"));
    let reader = rich();
    let (after, until) = game_version_reader(&reader).expect("tested");
    assert_eq!((after, until), ("version ", vec![' ']));
}

#[test]
fn the_other_stop_kinds_and_every_untested_element_are_handled() {
    assert!(matches!(
        stop_plan(&base()),
        StopPlan::ByImageOnSignal {
            grace_s: 30,
            signal: Signal::Sigterm
        }
    ));
    let not_saved = load_texts(
        &replace(
            BASE_FICHE,
            "world_saved = \"by_image_on_signal\"",
            "world_saved = \"not_saved\"",
        ),
        BASE_SETTINGS,
    )
    .expect("valide");
    assert!(matches!(
        stop_plan(&not_saved),
        StopPlan::NeedsDecision {
            reason: DecisionReason::NotSaved,
            ..
        }
    ));
    let untested = replace(
        &replace(
            &replace(
                RICH_FICHE,
                "[stop.sequence]\nstatus = \"tested\"",
                "[stop.sequence]\nstatus = \"untested\"",
            ),
            "[save]\nstatus = \"tested\"",
            "[save]\nstatus = \"untested\"",
        ),
        "log_contains = \"Done\"\nstatus = \"tested\"",
        "log_contains = \"Done\"\nstatus = \"untested\"",
    );
    let untested = replace(
        &untested,
        "until_any = [\" \"]\nscheme = \"dotted_numeric\"\nstatus = \"tested\"",
        "until_any = [\" \"]\nscheme = \"dotted_numeric\"\nstatus = \"untested\"",
    );
    let fiche = load_texts(&untested, RICH_SETTINGS).expect("valide");
    assert!(matches!(
        stop_plan(&fiche),
        StopPlan::NeedsDecision {
            reason: DecisionReason::UntestedSequence,
            ..
        }
    ));
    assert_eq!(save_plan(&fiche), None);
    assert_eq!(ready_sign(&fiche), None);
    assert!(game_version_reader(&fiche).is_none());
}
