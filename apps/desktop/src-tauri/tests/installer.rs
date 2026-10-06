//! Gardes du script de l'installateur NSIS (HRT-21, BR-CLIENT-006) : l'installateur n'est jamais
//! exécuté en test (il écrirait dans le registre du poste), mais son script est du texte versionné
//! et ses promesses se lisent : case décochée par défaut, aucune écriture sans que la page ait été
//! vue (silencieux et mise à jour automatique n'ont pas de page), même entrée `Run` que le réglage
//! de l'application. La compilation réelle du script est faite par la CI (`npm run tauri build`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use hearth_desktop_lib::domain::MINIMIZED_FLAG;

fn read(relative: &str) -> String {
    std::fs::read_to_string(format!("{}/{relative}", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .replace("\r\n", "\n")
}

fn hooks() -> String {
    read("installer/hooks.nsh")
}

/// Corps d'un bloc `Function nom` … `FunctionEnd`.
fn function_body(script: &str, name: &str) -> String {
    let start = script
        .find(&format!("Function {name}\n"))
        .unwrap_or_else(|| panic!("fonction {name} absente"));
    let rest = &script[start..];
    let end = rest.find("FunctionEnd").unwrap();
    rest[..end].to_owned()
}

/// Corps du crochet `!macro NSIS_HOOK_POSTINSTALL` … `!macroend`.
fn post_install(script: &str) -> String {
    let start = script.find("!macro NSIS_HOOK_POSTINSTALL").unwrap();
    let rest = &script[start..];
    rest[..rest.find("!macroend").unwrap()].to_owned()
}

fn define(script: &str, name: &str) -> String {
    let prefix = format!("!define {name} \"");
    let line = script
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("{name} non défini"));
    line[prefix.len()..line.len() - 1].to_owned()
}

#[test]
fn the_installer_writes_the_same_startup_entry_as_the_app() {
    let script = hooks();
    let config: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    // Le greffon autostart nomme l'entrée d'après le nom du produit et la place sous `Run`.
    assert_eq!(
        define(&script, "HEARTH_RUN_VALUE"),
        config["productName"].as_str().unwrap()
    );
    assert_eq!(
        define(&script, "HEARTH_RUN_KEY"),
        r"Software\Microsoft\Windows\CurrentVersion\Run"
    );
    assert_eq!(define(&script, "HEARTH_MINIMIZED_FLAG"), MINIMIZED_FLAG);
    assert_eq!(
        config["bundle"]["windows"]["nsis"]["installerHooks"],
        "installer/hooks.nsh"
    );
}

#[test]
fn the_checkbox_is_unchecked_unless_the_entry_already_exists() {
    let show = function_body(&hooks(), "HearthWelcomeShow");
    let first_check = show.find("NSD_Check}").expect("la case peut être cochée");
    // Aucune coche avant une condition : le défaut d'une installation neuve est décoché.
    let first_condition = show.find("${If}").unwrap();
    assert!(first_condition < first_check);
    // Les deux seules causes d'une case cochée : choix déjà fait à cette session, ou entrée existante.
    assert_eq!(show.matches("NSD_Check}").count(), 2);
    assert!(show.contains("$HearthAutostartShown = 1"));
    assert!(show.contains("$HearthAutostartWas = 1"));
    assert!(show.contains("$(hearthAutostartLabel)"));
    assert!(show.contains("$(hearthAutostartHelp)"));
}

#[test]
fn the_registry_is_only_touched_when_the_page_was_seen() {
    let script = hooks();
    let hook = post_install(&script);
    // Silencieux (/S) et mise à jour automatique (/UPDATE /P) n'affichent pas la page d'accueil :
    // `Shown` reste vide et le crochet ne fait RIEN. Toute écriture du registre est sous ce garde.
    let guard = hook.find("${If} $HearthAutostartShown = 1").unwrap();
    for write in ["WriteRegStr", "WriteRegBin", "DeleteRegValue"] {
        for (at, _) in hook.match_indices(write) {
            assert!(at > guard, "{write} hors du garde de la page");
        }
    }
    // Et rien d'autre dans le fichier n'écrit dans le registre (la lecture au démarrage de
    // l'interface est un `ReadRegStr`).
    let outside = script.replace(&hook, "");
    for write in ["WriteReg", "DeleteReg"] {
        assert!(
            !outside.contains(write),
            "{write} hors du crochet POSTINSTALL"
        );
    }
    // Cochée : l'entrée est écrite avec le drapeau ; décochée : elle est retirée.
    assert!(hook.contains("${HEARTH_MINIMIZED_FLAG}"));
    assert!(hook.contains("$HearthAutostartWanted = 1"));
}

#[test]
fn the_page_is_the_template_welcome_page_and_keeps_its_passive_skip() {
    let script = hooks();
    // Pas de rappel PRE : celui du modèle (SkipIfPassive) reste le seul, la page saute en passif.
    assert!(!script.contains("!define MUI_PAGE_CUSTOMFUNCTION_PRE"));
    assert!(script.contains("!define MUI_PAGE_CUSTOMFUNCTION_SHOW HearthWelcomeShow"));
    assert!(script.contains("!define MUI_PAGE_CUSTOMFUNCTION_LEAVE HearthWelcomeLeave"));
    // Le choix n'est retenu qu'en quittant la page (Annuler n'écrit donc rien).
    let leave = function_body(&script, "HearthWelcomeLeave");
    assert!(leave.contains("StrCpy $HearthAutostartShown 1"));
    assert!(leave.contains("NSD_GetState"));
    // Garde de compilation : le nom répété dans le script doit rester celui du produit.
    assert!(post_install(&script).contains(r#"!if "${PRODUCTNAME}" != "${HEARTH_RUN_VALUE}""#));
}

#[test]
fn the_texts_are_french_informal_and_complete() {
    let french = read("installer/French.nsh");
    let script = hooks();
    let added: Vec<&str> = french
        .lines()
        .filter(|line| {
            line.starts_with("LangString hearthAutostart")
                || line.starts_with("LangString hearthWelcomeText")
        })
        .collect();
    assert_eq!(added.len(), 3);
    for line in &added {
        assert!(!line.contains('\u{2014}'), "tiret cadratin : {line}");
        for formal in ["vous", "votre", "vos ", "Cliquez"] {
            assert!(!line.contains(formal), "vouvoiement ({formal}) : {line}");
        }
    }
    assert!(french.contains(
        r#"LangString hearthAutostartLabel ${LANG_FRENCH} "Lancer Hearth au démarrage de Windows""#
    ));
    // Toute clé de texte demandée par le script est définie.
    for (at, _) in script.match_indices("$(hearth") {
        let key: String = script[at + 2..].chars().take_while(|c| *c != ')').collect();
        assert!(
            french.contains(&format!("LangString {key} ${{LANG_FRENCH}}")),
            "texte {key} manquant"
        );
    }
}
