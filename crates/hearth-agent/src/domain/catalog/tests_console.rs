//! `[remote_console.use]` de la fiche Minecraft (HRT-267, ADR-0067, BR-CONSOLE-003, 004, 010, 011) : la fiche réelle se
//! charge avec ce qu'elle déclare, et des fiches piégées, dérivées d'UNE modification de la fiche réelle, sont refusées.

use super::embedded::load_all;
use super::loading::error::{FicheRefusee, Motif};
use super::loading::model::Fiche;
use super::loading::schema::fiche::{QuickMode, SensitiveTarget};
use super::{Origin, load};

const FICHE: &str = include_str!("../../../catalog/minecraft/fiche.toml");
const SETTINGS: &str = include_str!("../../../catalog/minecraft/settings.toml");

fn load_fiche(text: &str) -> Result<Fiche, FicheRefusee> {
    load(Origin::Embedded, text.as_bytes(), SETTINGS.as_bytes())
}

fn changed(from: &str, to: &str) -> String {
    assert_eq!(FICHE.matches(from).count(), 1, "{from}");
    FICHE.replace(from, to)
}

fn refusal(text: &str) -> String {
    match load_fiche(text) {
        Ok(_) => panic!("la fiche piégée a été acceptée"),
        Err(refusal) => match refusal.motif {
            Motif::ConsoleIncoherente { raison } => raison.to_owned(),
            other => panic!("autre motif : {other:?}"),
        },
    }
}

#[test]
fn the_embedded_minecraft_fiche_declares_what_hearth_may_do_with_its_console() {
    let (fiches, refused) = load_all();
    assert!(refused.is_empty());
    let fiche = fiches.into_iter().find(|f| f.id() == "minecraft").unwrap();
    let usage = fiche.file.remote_console.unwrap().console_use.expect("use");
    let refused: Vec<&str> = usage.refused.iter().map(|r| r.verb.as_str()).collect();
    assert_eq!(refused, ["stop", "restart", "reload"]);
    let sensitive: Vec<&str> = usage.sensitive.iter().map(|s| s.verb.as_str()).collect();
    assert_eq!(
        sensitive,
        [
            "op",
            "deop",
            "ban",
            "ban-ip",
            "pardon",
            "pardon-ip",
            "kick",
            "whitelist",
            "save-off",
            "execute",
            "function",
            "schedule",
            "datapack"
        ]
    );
    assert_eq!(usage.sensitive[0].target, SensitiveTarget::FirstArg);
    assert_eq!(usage.sensitive[3].target, SensitiveTarget::None);
    let players = usage.players.expect("joueurs");
    assert_eq!(
        (players.command.as_str(), players.reader.as_str()),
        ("list", "minecraft_list")
    );
    assert_eq!(usage.quick.len(), 3);
    assert_eq!(usage.quick[0].mode, QuickMode::Send);
    assert_eq!(usage.quick[2].mode, QuickMode::Fill);
    let before = usage.before_stop.expect("avant l'arrêt");
    assert_eq!(before.command, "save-all flush");
    assert_eq!(usage.reply_timeout_s, 10);
}

#[test]
fn a_fiche_without_a_use_block_loads_and_never_lets_hearth_in() {
    let cut = FICHE.split("\n[remote_console.use]").next().unwrap();
    let after = FICHE.split("[[agreement]]").nth(1).unwrap();
    let text = format!("{cut}\n[[agreement]]{after}");
    let fiche = load_fiche(&text).expect("fiche sans bloc use");
    assert!(fiche.file.remote_console.unwrap().console_use.is_none());
}

#[test]
fn a_trapped_console_block_is_refused_with_the_console_motif() {
    // Un verbe à la fois refusé et sensible.
    assert!(refusal(&changed("verb = \"op\"\n", "verb = \"stop\"\n")).contains("deux fois"));
    // Une raison hors de la liste fermée.
    assert!(
        refusal(&changed(
            "reason = \"breaks_server\"",
            "reason = \"parce que\""
        ))
        .contains("raison")
    );
    // Un verbe hors de la forme.
    assert!(refusal(&changed("verb = \"deop\"", "verb = \"De Op\"")).contains("verbe"));
    // Un emplacement autre que {joueur}.
    assert!(
        refusal(&changed(
            "Donner les droits d'opérateur à {joueur} ?",
            "Donner à {lieu} ?"
        ))
        .contains("emplacement")
    );
    // Un caractère à risque dans une phrase.
    assert!(
        refusal(&changed(
            "Il garde ces droits",
            "Il garde\u{202e} ces droits"
        ))
        .contains("risque")
    );
    // Une phrase vide.
    assert!(
        refusal(&changed(
            "sentence = \"{joueur} pourra de nouveau se connecter à ce serveur.\"",
            "sentence = \"\""
        ))
        .contains("240")
    );
    // Une commande rapide envoyée de classe sensible.
    assert!(
        refusal(&changed("text = \"save-all\"", "text = \"op Bram\"")).contains("classe libre")
    );
    // Une commande rapide d'un mod (espace de noms inconnu) est sensible par défaut : jamais envoyée sans mot de passe.
    assert!(
        refusal(&changed("text = \"save-all\"", "text = \"coolmod:wipe\""))
            .contains("classe libre")
    );
    // Un texte non ASCII, ou de plus de 64 caractères.
    assert!(
        refusal(&changed(
            "text = \"time set day\"",
            "text = \"time set d\u{e9}j\u{e0}\""
        ))
        .contains("ASCII")
    );
    let long = "a".repeat(65);
    assert!(
        refusal(&changed(
            "text = \"time set day\"",
            &format!("text = \"{long}\"")
        ))
        .contains("64")
    );
    // La commande des joueurs de classe sensible.
    assert!(
        refusal(&changed("command = \"list\"", "command = \"kick x\"")).contains("classe libre")
    );
    // Un lecteur inconnu.
    assert!(
        refusal(&changed(
            "reader = \"minecraft_list\"",
            "reader = \"autre\""
        ))
        .contains("lecteur")
    );
    // Le délai de réponse hors de 2 à 30.
    assert!(refusal(&changed("reply_timeout_s = 10", "reply_timeout_s = 1")).contains("30"));
    assert!(refusal(&changed("reply_timeout_s = 10", "reply_timeout_s = 31")).contains("30"));
    // Le délai d'avant l'arrêt hors de 5 à 120.
    assert!(refusal(&changed("timeout_s = 60", "timeout_s = 4")).contains("120"));
    // La commande d'avant l'arrêt de classe refusée.
    assert!(
        refusal(&changed(
            "command = \"save-all flush\"",
            "command = \"stop\""
        ))
        .contains("classe libre")
    );
}

#[test]
fn more_than_six_quick_commands_or_a_repeated_id_are_refused() {
    let mut text = FICHE.to_owned();
    for n in 0..4 {
        text = text.replace(
            "\n[remote_console.use.before_stop]",
            &format!("\n[[remote_console.use.quick]]\nid = \"q{n}\"\nlabel = \"Un\"\ntext = \"list\"\nmode = \"send\"\n\n[remote_console.use.before_stop]"),
        );
    }
    assert!(refusal(&text).contains("6 commandes"));
    assert!(refusal(&changed("id = \"day\"", "id = \"save\"")).contains("deux fois"));
}

#[test]
fn use_is_refused_on_a_protocol_hearth_does_not_speak() {
    assert!(
        refusal(&changed(
            "protocol = \"source_rcon\"",
            "protocol = \"telnet_line\""
        ))
        .contains("source_rcon")
    );
}
