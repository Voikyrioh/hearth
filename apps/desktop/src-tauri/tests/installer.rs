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

/// Bloc `${If} condition` … `${EndIf}` correspondant (profondeur comptée), bornes comprises.
fn if_block(text: &str, opening: &str) -> String {
    let start = text
        .find(opening)
        .unwrap_or_else(|| panic!("{opening} absent"));
    let mut depth = 0_i32;
    let mut at = start;
    while at < text.len() {
        let rest = &text[at..];
        if rest.starts_with("${If}") || rest.starts_with("${IfNot}") {
            depth += 1;
        } else if rest.starts_with("${EndIf}") {
            depth -= 1;
            if depth == 0 {
                return text[start..at + "${EndIf}".len()].to_owned();
            }
        }
        at += 1;
    }
    panic!("bloc {opening} non fermé");
}

#[test]
fn the_registry_is_only_touched_when_the_page_was_seen() {
    let script = hooks();
    let hook = post_install(&script);
    // Silencieux (/S) et mise à jour automatique (/UPDATE /P) n'affichent pas la page d'accueil :
    // `Shown` reste vide et le crochet ne fait RIEN. Toute écriture du registre est DANS le
    // bloc gardé par `Shown`, pas seulement après lui.
    let guarded = if_block(&hook, "${If} $HearthAutostartShown = 1");
    for write in ["WriteRegStr", "WriteRegBin", "DeleteRegValue"] {
        assert_eq!(
            hook.matches(write).count(),
            guarded.matches(write).count(),
            "{write} hors du bloc gardé par la page"
        );
        assert!(guarded.contains(write));
    }
    // Rien d'autre dans le fichier n'écrit dans le registre (la lecture au démarrage de
    // l'interface ne fait que lire).
    let outside = script.replace(&hook, "");
    for write in ["WriteReg", "DeleteReg", "RegSetValue", "RegDeleteValue"] {
        assert!(
            !outside.contains(write),
            "{write} hors du crochet POSTINSTALL"
        );
    }
    // Cochée : écrite ; décochée : retirée seulement si le démarrage était activé.
    assert!(guarded.contains("${If} $HearthAutostartWanted = 1"));
    assert!(guarded.contains("${ElseIf} $HearthAutostartWas = 1"));
    // Valeur strictement celle du greffon : `chemin --minimized`, sans guillemets.
    assert!(guarded.contains(r#""$INSTDIR\${MAINBINARYNAME}.exe ${HEARTH_MINIMIZED_FLAG}""#));
}

#[test]
fn the_initial_state_follows_the_task_manager_like_the_plugin() {
    let script = hooks();
    let init = function_body(&script, "HearthGuiInit");
    // « Activé » = entrée présente ET non désactivée dans le Gestionnaire des tâches.
    assert!(init.contains("Call HearthTaskManagerEnabled"));
    assert!(init.contains("StrCpy $HearthAutostartWas 1"));
    let read = function_body(&script, "HearthTaskManagerEnabled");
    assert!(read.contains("${HEARTH_STARTUP_APPROVED_KEY}"));
    assert!(
        read.contains("$2 >= 8"),
        "au moins 8 octets, comme auto-launch"
    );
    assert!(read.contains("StrCpy $R0 0"));
}

/// Rectangles `120u 118u 195u 12u` des contrôles créés par la page : (haut, bas, gauche, droite).
fn created_rectangles(script: &str) -> Vec<(u32, u32, u32, u32)> {
    let show = function_body(script, "HearthWelcomeShow");
    show.lines()
        .filter(|line| line.contains("${NSD_Create"))
        .map(|line| {
            let numbers: Vec<u32> = line
                .split_whitespace()
                .filter_map(|word| word.strip_suffix('u')?.parse().ok())
                .collect();
            assert_eq!(numbers.len(), 4, "{line}");
            (
                numbers[1],
                numbers[1] + numbers[3],
                numbers[0],
                numbers[0] + numbers[2],
            )
        })
        .collect()
}

#[test]
fn the_welcome_page_controls_do_not_overlap_and_fit_the_dialog() {
    let script = hooks();
    let boxes = created_rectangles(&script);
    assert_eq!(boxes.len(), 3, "texte d'accueil, case, aide");
    for (index, a) in boxes.iter().enumerate() {
        for b in &boxes[index + 1..] {
            let separate = a.1 <= b.0 || b.1 <= a.0 || a.3 <= b.2 || b.3 <= a.2;
            assert!(separate, "{a:?} recouvre {b:?}");
        }
        // Page d'accueil de MUI2 : 193u de haut, 315u de large (Welcome.nsh).
        assert!(a.1 <= 193 && a.3 <= 315, "{a:?} déborde de la page");
    }
    // Le contrôle de texte du modèle (120u 45u 195u 130u, fond opaque) est laissé vide : c'est
    // le texte redessiné ci-dessus qui porte la phrase, sous le titre (qui finit à 48u).
    assert!(script.contains(r#"!define MUI_WELCOMEPAGE_TEXT " ""#));
    assert!(boxes.iter().all(|b| b.0 >= 48));
    // nsDialogs empile les contrôles du dessous vers le dessus dans l'ordre INVERSE de leur
    // création : ce contrôle vide, opaque, recouvrait la case (constaté sur la capture de la CI).
    let show = function_body(&script, "HearthWelcomeShow");
    assert!(show.contains(r#"FindWindow $R2 "Static" " " $R1"#));
    assert!(show.contains("ShowWindow $R2 0"));
}

#[test]
fn the_template_read_is_the_one_of_the_locked_tool_version() {
    // Le script dépend de l'ordre des pages, de `SkipIfPassive` et des crochets du modèle NSIS
    // embarqué dans @tauri-apps/cli, relu à la version 2.12.1. Une montée de version doit
    // forcer une nouvelle lecture de installer.nsi (puis ce test et l'ADR-0026).
    let lock: serde_json::Value = serde_json::from_str(&read("../package-lock.json")).unwrap();
    assert_eq!(
        lock["packages"]["node_modules/@tauri-apps/cli"]["version"], "2.12.1",
        "relire le modèle NSIS de la nouvelle version (page d'accueil, SkipIfPassive,          NSIS_HOOK_POSTINSTALL), puis mettre ce test à jour"
    );
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
