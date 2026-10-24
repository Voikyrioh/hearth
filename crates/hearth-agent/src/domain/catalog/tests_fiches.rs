//! Chargeur réel : fiches valides, fiches piégées (une par invariant et par motif), fiche Terraria embarquée,
//! `ids.lock`. Chaque fiche piégée est dérivée d'une fiche valide par UNE modification, et doit être refusée avec
//! LE bon motif typé, pas une erreur générique.

use super::embedded::{EMBEDDED, load_all};
use super::loading::error::{FicheRefusee, Fichier, Motif};
use super::loading::ids_lock::{self, Violation};
use super::loading::limits::{MAX_FILES_BYTES, MAX_SETTINGS};
use super::loading::loader::{Origin, load};
use super::loading::schema::fiche::Status;
use super::stop_plan::{game_version_reader, ready_sign};
use super::test_support::*;

fn bad(fiche: &str, settings: &str) -> FicheRefusee {
    match load_texts(fiche, settings) {
        Ok(_) => panic!("la fiche piégée a été acceptée"),
        Err(refusal) => refusal,
    }
}

fn bad_fiche(from: &str, to: &str) -> FicheRefusee {
    bad(&replace(BASE_FICHE, from, to), BASE_SETTINGS)
}

fn bad_fiche_plus(extra: &str) -> FicheRefusee {
    bad(&format!("{BASE_FICHE}\n{extra}\n"), BASE_SETTINGS)
}

fn bad_settings(from: &str, to: &str) -> FicheRefusee {
    bad(BASE_FICHE, &replace(BASE_SETTINGS, from, to))
}

fn bad_settings_plus(extra: &str) -> FicheRefusee {
    bad(BASE_FICHE, &format!("{BASE_SETTINGS}\n{extra}\n"))
}

fn bad_rich(from: &str, to: &str) -> FicheRefusee {
    bad(&replace(RICH_FICHE, from, to), RICH_SETTINGS)
}

fn bad_rich_settings(from: &str, to: &str) -> FicheRefusee {
    bad(RICH_FICHE, &replace(RICH_SETTINGS, from, to))
}

/// Le motif obtenu, pour un message d'échec lisible.
fn motif(refusal: &FicheRefusee) -> &Motif {
    &refusal.motif
}

fn unknown_field(refusal: &FicheRefusee, name: &str) {
    assert_eq!(
        motif(refusal),
        &Motif::ChampInconnu { champ: name.into() },
        "champ `{name}` : {refusal}"
    );
}

// ---- Fiches valides ----------------------------------------------------------------------------------------

#[test]
fn the_minimal_and_the_rich_fiches_load() {
    let base = base();
    assert_eq!(base.id(), "demo");
    let rich = rich();
    assert_eq!(rich.settings.len(), 13);
    assert_eq!(rich.retired.len(), 1);
    assert_eq!(rich.file.variant.len(), 2);
}

#[test]
fn the_bounds_themselves_are_accepted() {
    // 400 réglages (la borne) : acceptés.
    let mut settings = String::from("[[theme]]\nid = \"game\"\nlabel = \"Partie\"\norder = 1\n");
    for i in 0..MAX_SETTINGS {
        settings.push_str(&format!(
            "\n[[setting]]\nid = \"s{i}\"\nkey = \"k{i}\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Réglage {i}\"\nhelp = \"Aide.\"\ntype = \"int\"\nchange = \"anytime\"\nsource = \"partial\"\n"
        ));
    }
    load_texts(BASE_FICHE, &settings).expect("400 réglages passent");
    // Six étapes d'arrêt (la borne) : acceptées.
    let steps = "[[stop.sequence.step]]\nkind = \"wait\"\nseconds = 1\n\n".repeat(5);
    let fiche = replace(
        &replace(
            BASE_FICHE,
            "world_saved = \"by_image_on_signal\"",
            "world_saved = \"by_sequence\"",
        ),
        "[resources]",
        &format!(
            "[stop.sequence]\nstatus = \"untested\"\n\n{steps}[[stop.sequence.step]]\nkind = \"stdin_line\"\nline = \"exit\"\nquits = true\n\n[resources]"
        ),
    );
    load_texts(&fiche, BASE_SETTINGS).expect("6 étapes passent");
}

// ---- Format fermé -------------------------------------------------------------------------------------------

#[test]
fn format_closed_an_unknown_field_is_refused_at_every_level() {
    for field in [
        "privileged",
        "cap_add",
        "network_mode",
        "pid",
        "devices",
        "security_opt",
        "command",
        "entrypoint",
        "host_path",
        "script",
        "regex",
        "docker_socket",
        "user",
        "volumes",
        "mounts",
    ] {
        let refusal = bad_fiche(
            "status = \"preview\"",
            &format!("status = \"preview\"\n{field} = true"),
        );
        unknown_field(&refusal, field);
        assert_eq!(refusal.fichier, Fichier::Fiche);
        assert!(refusal.ligne.is_some(), "la ligne du champ est connue");
    }
    // Dans une sous-table du canal, une étape, le dossier, le port, l'image, les ressources.
    unknown_field(
        &bad_fiche("quote = \"none\"", "quote = \"none\"\nextra = 1"),
        "extra",
    );
    unknown_field(
        &bad_fiche(
            "tag = \"1.2.3\"",
            "tag = \"1.2.3\"\nregistry = \"evil.example\"",
        ),
        "registry",
    );
    unknown_field(
        &bad_fiche(
            "target = \"/data/world\"",
            "target = \"/data/world\"\nsource = \"/etc\"",
        ),
        "source",
    );
    unknown_field(
        &bad_fiche(
            "mapping = \"nat\"",
            "mapping = \"nat\"\nhost_network = true",
        ),
        "host_network",
    );
    unknown_field(
        &bad_fiche(
            "cpu = { evidence = \"unmeasured\" }",
            "cpu = { evidence = \"unmeasured\", limit = 1 }",
        ),
        "limit",
    );
    unknown_field(
        &bad_rich(
            "line = \"save\"\nquits = false",
            "line = \"save\"\nquits = false\nshell = true",
        ),
        "shell",
    );
    unknown_field(
        &bad_rich("sentence = \"J'ai lu", "extra = 1\nsentence = \"J'ai lu"),
        "extra",
    );
    unknown_field(
        &bad_rich(
            "accepted_value = \"TRUE\"",
            "accepted_value = \"TRUE\"\nforce = true",
        ),
        "force",
    );
    // Dans le fichier de réglages.
    unknown_field(
        &bad_settings(
            "help = \"Le nom du serveur.\"",
            "help = \"Le nom du serveur.\"\nregex = \"x\"",
        ),
        "regex",
    );
    unknown_field(
        &bad_settings(
            "arg = { flag = \"-tick\", form = \"flag_value\" }",
            "arg = { flag = \"-tick\", form = \"flag_value\", raw = \"x\" }",
        ),
        "raw",
    );
    unknown_field(
        &bad_rich_settings(
            "imposed = { value = \"true\",",
            "imposed = { exec = \"x\", value = \"true\",",
        ),
        "exec",
    );
}

#[test]
fn format_closed_an_unknown_kind_or_value_is_refused() {
    assert_eq!(
        *motif(&bad_fiche("kind = \"args\"", "kind = \"exec\"")),
        Motif::GenreDeCanalInconnu
    );
    assert_eq!(
        *motif(&bad_rich("kind = \"wait_log\"", "kind = \"exec\"")),
        Motif::GenreDEtapeInconnu
    );
    assert_eq!(
        *motif(&bad_rich("kind = \"wait_log\"", "kind = \"shell\"")),
        Motif::GenreDEtapeInconnu
    );
    assert_eq!(
        *motif(&bad_fiche("status = \"preview\"", "status = \"draft\"")),
        Motif::VarianteInconnue
    );
    assert_eq!(
        *motif(&bad_fiche("protocol = \"tcp\"", "protocol = \"sctp\"")),
        Motif::VarianteInconnue
    );
    assert_eq!(
        *motif(&bad_fiche("signal = \"SIGTERM\"", "signal = \"SIGKILL\"")),
        Motif::VarianteInconnue
    );
    assert_eq!(
        *motif(&bad_fiche("platform = \"linux\"", "platform = \"windows\"")),
        Motif::VarianteInconnue
    );
    assert_eq!(
        *motif(&bad_settings("type = \"int\"", "type = \"regex\"")),
        Motif::VarianteInconnue
    );
}

#[test]
fn format_closed_a_missing_field_a_wrong_version_and_broken_files_are_refused() {
    assert_eq!(
        *motif(&bad_fiche(
            "summary = \"Une fiche de démonstration.\"\n",
            ""
        )),
        Motif::ChampManquant {
            champ: "summary".into()
        }
    );
    assert_eq!(
        *motif(&bad_fiche("format = 2", "format = 1")),
        Motif::FormatInattendu { trouve: 1 }
    );
    assert!(matches!(
        motif(&bad("ceci n'est pas du TOML [[", BASE_SETTINGS)),
        Motif::Syntaxe { .. }
    ));
    assert!(matches!(
        load(Origin::Embedded, b"\xff\xfe", BASE_SETTINGS.as_bytes())
            .expect_err("refusée")
            .motif,
        Motif::Syntaxe { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche("uid = 1000", "uid = \"root\"")),
        Motif::Syntaxe { .. }
    ));
    let remote = load(
        Origin::Remote,
        BASE_FICHE.as_bytes(),
        BASE_SETTINGS.as_bytes(),
    )
    .expect_err("refusée");
    assert_eq!(remote.motif, Motif::OrigineNonSupportee);
}

// ---- I1 : une seule voie, plan fermé -----------------------------------------------------------------------------

#[test]
fn i1_one_way_closed_plan_refuses_image_and_port_traps() {
    for tag in ["latest", "LATEST", "", "a b", "1.0;rm"] {
        assert_eq!(
            *motif(&bad_fiche("tag = \"1.2.3\"", &format!("tag = \"{tag}\""))),
            Motif::ImageSansVersion,
            "{tag:?}"
        );
    }
    for repo in [
        "Example/Demo",
        "https://evil.example/demo",
        "example/demo:tag",
        "/demo",
        "demo/",
        "a//b",
        "",
    ] {
        assert_eq!(
            *motif(&bad_fiche(
                "repository = \"example/demo\"",
                &format!("repository = \"{repo}\"")
            )),
            Motif::DepotInvalide,
            "{repo:?}"
        );
    }
    assert_eq!(
        *motif(&bad_fiche(
            "tag = \"1.2.3\"",
            "tag = \"1.2.3\"\ndigest = \"sha256:xyz\""
        )),
        Motif::EmpreinteInvalide
    );
    assert_eq!(
        *motif(&bad_fiche(
            "tag = \"1.2.3\"",
            &format!("tag = \"1.2.3\"\ndigest = \"sha256:{}\"", "G".repeat(64))
        )),
        Motif::EmpreinteInvalide
    );
    assert_eq!(
        *motif(&bad_fiche("status = \"preview\"", "status = \"published\"")),
        Motif::PublieeSansEmpreinte
    );
    for port in [80u16, 22, 443, 1023] {
        assert_eq!(
            *motif(&bad_fiche(
                "container = 7777",
                &format!("container = {port}")
            )),
            Motif::PortReserve { port }
        );
        assert_eq!(
            *motif(&bad_fiche(
                "default_host = 7777",
                &format!("default_host = {port}")
            )),
            Motif::PortReserve { port }
        );
    }
    assert_eq!(
        *motif(&bad_fiche_plus(
            "[[port]]\nid = \"again\"\ncontainer = 7777\nprotocol = \"tcp\"\nrole = \"query\"\npublish = \"optional\"\nmapping = \"nat\""
        )),
        Motif::PortEnDouble { port: 7777 }
    );
    // Une variante ne change que l'étiquette du MÊME dépôt : aucun champ pour un autre dépôt.
    unknown_field(
        &bad_rich(
            "image_tag = \"bedrock-2.0\"",
            "image_tag = \"bedrock-2.0\"\nimage_repository = \"evil/x\"",
        ),
        "image_repository",
    );
    assert_eq!(
        *motif(&bad_rich(
            "image_tag = \"bedrock-2.0\"",
            "image_tag = \"latest\""
        )),
        Motif::ImageSansVersion
    );
}

// ---- I2 : tous les montages sont des dossiers gérés ----------------------------------------------------------------

#[test]
fn i2_every_mount_is_a_managed_folder_no_host_path_exists() {
    for field in ["source", "host_path", "bind", "path", "file", "volume"] {
        unknown_field(
            &bad_fiche(
                "backup = true\n\n[[folder]]\nid = \"config\"",
                &format!("backup = true\n{field} = \"/etc/shadow\"\n\n[[folder]]\nid = \"config\""),
            ),
            field,
        );
    }
    // Alphabet fermé : rien qui ait un sens dans une description de montage (`source:cible:options`, `,`, `=`).
    for target in [
        "data",
        "/",
        "/a/../b",
        "/a//b",
        "/a\\b",
        "",
        "./x",
        "/data:rw",
        "/x,bind-propagation=shared",
        "/a b",
        "/a=b",
        "/a$b",
        "/a;b",
    ] {
        let refusal = bad_fiche(
            "target = \"/data/world\"",
            &format!("target = \"{}\"", target.replace('\\', "\\\\")),
        );
        assert!(
            matches!(motif(&refusal), Motif::CheminInvalide { .. }),
            "{target:?} : {refusal}"
        );
    }
    assert_eq!(
        *motif(&bad_fiche(
            "target = \"/data/config\"",
            "target = \"/data/world\""
        )),
        Motif::CibleEnDouble
    );
    for path in [
        "/etc/passwd",
        "../x",
        "a/../b",
        "a//b",
        "",
        "a b",
        "a\\\\b",
        "./x",
    ] {
        let refusal = bad_fiche("path = \"server.cfg\"", &format!("path = \"{path}\""));
        assert!(
            matches!(motif(&refusal), Motif::CheminInvalide { .. }),
            "{path:?} : {refusal}"
        );
    }
    assert_eq!(
        *motif(&bad_fiche("folder = \"config\"", "folder = \"nowhere\"")),
        Motif::DossierInconnu {
            id: "nowhere".into()
        }
    );
    assert!(matches!(
        motif(&bad_fiche("id = \"world\"\nrole", "id = \"World\"\nrole")),
        Motif::IdentifiantInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche("id = \"config\"\nrole", "id = \"world\"\nrole")),
        Motif::IdentifiantEnDouble { .. }
    ));
}

// ---- I3 : aucun texte n'atteint un interpréteur --------------------------------------------------------------------

fn arg_setting(proposed: &str) -> String {
    format!(
        "[[setting]]\nid = \"map\"\nkey = \"-map\"\nchannel = \"args\"\narg = {{ flag = \"-map\", form = \"flag_value\" }}\ntheme = \"game\"\nlevel = \"common\"\nlabel = \"Carte\"\nhelp = \"La carte.\"\ntype = \"text\"\ncharset = \"arg_safe\"\nmax_len = 20\nproposed = \"{proposed}\"\nchange = \"anytime\"\nsource = \"partial\"\n"
    )
}

#[test]
fn i3_no_text_reaches_an_interpreter_arguments_orders_and_derived_values() {
    // La valeur d'un argument : le message cite le caractère fautif, jamais la valeur.
    for bad_char in [
        ' ', ';', '$', '|', '&', '<', '>', '(', ')', '\\', '*', '?', '`', '\'', '!', '{', '~', '=',
        '/', '#',
    ] {
        let value = format!(
            "ab{}cd",
            if bad_char == '\\' {
                "\\\\".to_string()
            } else {
                bad_char.to_string()
            }
        );
        let refusal = bad_settings_plus(&arg_setting(&value));
        assert_eq!(
            *motif(&refusal),
            Motif::ArgumentHorsAlphabet {
                caractere: bad_char
            },
            "{bad_char:?}"
        );
        let message = refusal.message_pour("demo");
        assert!(
            !message.contains("abcd") && !message.contains(&format!("ab{bad_char}cd")),
            "le message cite la valeur : {message}"
        );
    }
    for leading in ["-x", "+x"] {
        assert!(
            matches!(
                motif(&bad_settings_plus(&arg_setting(leading))),
                Motif::ArgumentHorsAlphabet { .. }
            ),
            "{leading}"
        );
    }
    load_texts(
        BASE_FICHE,
        &format!("{BASE_SETTINGS}\n{}", arg_setting("Monde-1_2.3:4")),
    )
    .expect("l'alphabet fermé passe");
    // Le nom d'un argument est un littéral de la fiche.
    for flag in ["tick", "-a b", "-$x", "- x", "-a;b", "--", "-"] {
        assert_eq!(
            *motif(&bad_settings(
                "arg = { flag = \"-tick\"",
                &format!("arg = {{ flag = \"{flag}\"")
            )),
            Motif::ArgumentNonLitteral,
            "{flag:?}"
        );
    }
    // Jamais un secret en argument.
    let secret_arg = "[[setting]]\nid = \"tok\"\nkey = \"-token\"\nchannel = \"args\"\narg = { flag = \"-token\", form = \"flag_value\" }\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Jeton\"\nhelp = \"Le jeton.\"\ntype = \"secret\"\nsecret_kind = \"server_token\"\nmax_len = 40\nchange = \"anytime\"\nsource = \"partial\"\n";
    assert_eq!(
        *motif(&bad_settings_plus(secret_arg)),
        Motif::SecretEnArgument
    );
    // Un texte libre ne passe pas par un argument.
    assert!(matches!(
        motif(&bad_settings(
            "type = \"int\"\nmin = 1\nmax = 128\nproposed = \"20\"",
            "type = \"text\"\ncharset = \"line\"\nmax_len = 10"
        )),
        Motif::ChampIncoherent { .. }
    ));
    // Valeur dérivée : un préfixe, UN réglage, un suffixe.
    unknown_field(
        &bad_fiche_plus("[[env]]\nname = \"X\"\nderive = { from = \"name\", from2 = \"tick\" }"),
        "from2",
    );
    assert!(
        matches!(
            motif(&bad_fiche_plus(
                "[[env]]\nname = \"X\"\nderive = { from = \"name\" }"
            )),
            Motif::DeriveInvalide { .. }
        ),
        "un texte libre ne borne pas le résultat"
    );
    assert!(matches!(
        motif(&bad_fiche_plus(
            "[[env]]\nname = \"X\"\nderive = { from = \"tick\" }"
        )),
        Motif::DeriveInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche_plus(
            "[[env]]\nname = \"X\"\nderive = { from = \"absent\" }"
        )),
        Motif::ReferenceInconnue { .. }
    ));
    // I3 : une valeur dérivée imposée sur le canal des arguments ne laisse passer aucun préfixe libre.
    let with_source = format!(
        "{BASE_SETTINGS}
{}",
        arg_setting("ok")
    );
    let derived_on = |tick_or_dst: &str, prefix: &str| {
        let settings = if tick_or_dst == "int" {
            replace(
                &with_source,
                "proposed = \"20\"",
                &format!(
                    "imposed = {{ derive = {{ prefix = \"{prefix}\", from = \"map\" }}, why = \"Test.\" }}"
                ),
            )
        } else {
            let dst = arg_setting("ok")
                .replace("\"map\"", "\"dst\"")
                .replace("-map", "-dst")
                .replace(
                    "proposed = \"ok\"",
                    &format!("imposed = {{ derive = {{ prefix = \"{prefix}\", from = \"map\" }}, why = \"Test.\" }}"),
                );
            format!(
                "{with_source}
{dst}"
            )
        };
        bad(BASE_FICHE, &settings)
    };
    for prefix in ["--config=/monde/x ", "/m/", "a b", "$x", "=", "-x", "a;b"] {
        assert!(
            matches!(
                motif(&derived_on("int", prefix)),
                Motif::DeriveInvalide { .. }
            ),
            "réglage entier, préfixe {prefix:?}"
        );
        assert!(
            matches!(
                motif(&derived_on("text", prefix)),
                Motif::DeriveInvalide { .. }
            ),
            "réglage texte, préfixe {prefix:?}"
        );
    }
    // Hors piège : un préfixe dans l'alphabet fermé passe.
    let ok = {
        let dst = arg_setting("ok")
            .replace("\"map\"", "\"dst\"")
            .replace("-map", "-dst")
            .replace(
                "proposed = \"ok\"",
                "imposed = { derive = { prefix = \"m.\", from = \"map\" }, why = \"Test.\" }",
            );
        load_texts(
            BASE_FICHE,
            &format!(
                "{with_source}
{dst}"
            ),
        )
    };
    assert!(ok.is_ok(), "{:?}", ok.err());
    // Ordre envoyé au jeu : littéral, 64 caractères ASCII imprimables au plus.
    for (line, label) in [
        ("x".repeat(65), "65 caractères"),
        ("sauvé".to_owned(), "non ASCII"),
        ("save\\nexit".to_owned(), "retour à la ligne"),
        (String::new(), "vide"),
        (" save".to_owned(), "espace de tête"),
        ("save\\u0007".to_owned(), "caractère de contrôle"),
    ] {
        let refusal = bad_rich(
            "line = \"save\"\nquits = false",
            &format!("line = \"{line}\"\nquits = false"),
        );
        assert!(
            matches!(motif(&refusal), Motif::OrdreInvalide { .. }),
            "{label} : {refusal}"
        );
    }
    // Valeur fixe d'une variable : pas de caractère de contrôle.
    assert!(matches!(
        motif(&bad_fiche_plus(
            "[[env]]\nname = \"X\"\nvalue = \"a\\u0007b\""
        )),
        Motif::ValeurFixeInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche_plus("[[env]]\nname = \"x-y\"\nvalue = \"1\"")),
        Motif::IdentifiantInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche_plus(
            "[[env]]\nname = \"X\"\nvalue = \"1\"\nowner = \"uid\""
        )),
        Motif::ChampIncoherent { .. }
    ));
}

// ---- I4 : aucune exécution ---------------------------------------------------------------------------------------

#[test]
fn i4_nothing_is_executed_no_step_kind_runs_a_command() {
    assert_eq!(
        *motif(&bad_rich("kind = \"wait_log\"", "kind = \"exec\"")),
        Motif::GenreDEtapeInconnu
    );
    for field in ["command", "exec", "run", "script", "shell", "args"] {
        unknown_field(
            &bad_rich(
                "line = \"save\"\nquits = false",
                &format!("line = \"save\"\nquits = false\n{field} = \"rm -rf /\""),
            ),
            field,
        );
    }
    // Une étape ne mélange pas les champs de deux genres.
    assert!(matches!(
        motif(&bad_rich(
            "line = \"save\"\nquits = false",
            "line = \"save\"\nquits = false\ncontains = \"x\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "contains = \"Saved\"\ntimeout_s = 30\nrequired = true",
            "contains = \"Saved\"\ntimeout_s = 30\nrequired = true\nline = \"x\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    // Séquence incohérente.
    assert!(
        matches!(
            motif(&bad_rich(
                "line = \"save\"\nquits = false",
                "line = \"save\"\nquits = true"
            )),
            Motif::SequenceIncoherente { .. }
        ),
        "un ordre de sortie n'est pas suivi d'une autre étape"
    );
    assert!(
        matches!(
            motif(&bad_rich(
                "[save]\nstatus = \"tested\"\n\n[[save.step]]\nkind = \"stdin_line\"\nline = \"save\"\nquits = false",
                "[save]\nstatus = \"tested\"\n\n[[save.step]]\nkind = \"stdin_line\"\nline = \"save\"\nquits = true"
            )),
            Motif::SequenceIncoherente { .. }
        ),
        "un enregistrement à chaud ne quitte jamais"
    );
    assert!(matches!(
        motif(&bad_fiche(
            "world_saved = \"by_image_on_signal\"",
            "world_saved = \"by_sequence\""
        )),
        Motif::SequenceIncoherente { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "world_saved = \"by_sequence\"",
            "world_saved = \"by_image_on_signal\""
        )),
        Motif::SequenceIncoherente { .. }
    ));
    // Bornes de durée d'une étape.
    assert!(matches!(
        motif(&bad_rich(
            "timeout_s = 30\nrequired = true",
            "timeout_s = 301\nrequired = true"
        )),
        Motif::BorneInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "timeout_s = 30\nrequired = true",
            "timeout_s = 0\nrequired = true"
        )),
        Motif::BorneInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche("grace_s = 30", "grace_s = 9")),
        Motif::BorneInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_fiche("grace_s = 30", "grace_s = 601")),
        Motif::BorneInvalide { .. }
    ));
    // Les champs manquants d'une étape sont refusés plutôt que remplis par défaut.
    assert_eq!(
        *motif(&bad_rich(
            "line = \"save\"\nquits = false",
            "line = \"save\""
        )),
        Motif::ChampManquant {
            champ: "quits".into()
        }
    );
}

// ---- I5 : aucune adresse contactée --------------------------------------------------------------------------------

#[test]
fn i5_no_address_is_contacted_https_only() {
    for url in [
        "http://example.org/eula",
        "ftp://example.org",
        "javascript:alert(1)",
        "//example.org",
        "example.org",
    ] {
        assert_eq!(
            *motif(&bad_rich("https://example.org/eula", url)),
            Motif::AdresseNonHttps,
            "{url}"
        );
    }
    for url in [
        "https://",
        "https://u:p@example.org/",
        "https://exa mple.org",
        "https://example.org/\\\"x",
        "https://exa<mple.org",
    ] {
        assert_eq!(
            *motif(&bad_rich("https://example.org/eula", url)),
            Motif::AdresseInvalide,
            "{url}"
        );
    }
    assert_eq!(
        *motif(&bad_rich(
            "bytes = 1073741824\nevidence = { publisher = \"https://example.org/size\" }",
            "bytes = 1073741824\nevidence = { publisher = \"http://example.org/size\" }"
        )),
        Motif::AdresseNonHttps
    );
    assert_eq!(
        *motif(&bad_rich(
            "disk = { install_bytes = 1073741824, evidence = { publisher = \"https://example.org/size\" } }",
            "disk = { install_bytes = 1073741824, evidence = { publisher = \"http://example.org/size\" } }"
        )),
        Motif::AdresseNonHttps
    );
    let token = |url: &str| {
        format!(
            "[[setting]]\nid = \"tok\"\nkey = \"token\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Jeton\"\nhelp = \"Le jeton.\"\ntype = \"secret\"\nsecret_kind = \"server_token\"\nmax_len = 40\nobtain_url = \"{url}\"\nchange = \"anytime\"\nsource = \"partial\"\n"
        )
    };
    assert_eq!(
        *motif(&bad_settings_plus(&token("http://example.org/tokens"))),
        Motif::AdresseNonHttps
    );
    load_texts(
        BASE_FICHE,
        &format!("{BASE_SETTINGS}\n{}", token("https://example.org/tokens")),
    )
    .expect("https passe");
}

// ---- I6 : ni expression régulière, ni gabarit --------------------------------------------------------------------------

#[test]
fn i6_neither_regular_expression_nor_template_signs_and_versions_are_literals() {
    // Un repère qui ressemble à une expression régulière reste un LITTÉRAL cherché tel quel.
    let regexy = ".*[a-z]+(x|y)\\\\d{3}$";
    let fiche = load_texts(
        &replace(
            RICH_FICHE,
            "log_contains = \"Done\"",
            &format!("log_contains = \"{regexy}\""),
        ),
        RICH_SETTINGS,
    )
    .expect("un littéral est un littéral");
    assert_eq!(ready_sign(&fiche), Some(".*[a-z]+(x|y)\\d{3}$"));
    assert!(super::loading::value::log_line_contains(
        "x .*[a-z]+(x|y)\\d{3}$ y",
        ready_sign(&fiche).expect("signe")
    ));
    assert!(!super::loading::value::log_line_contains(
        "abc xyz123",
        ready_sign(&fiche).expect("signe")
    ));
    // Version du jeu : le texte entre deux littéraux, un seul caractère de fin.
    for until in [
        "[\"ab\"]",
        "[]",
        "[\"\"]",
        "[\" \", \"x\", \"y\", \"z\", \"a\", \"b\", \"c\", \"d\", \"e\"]",
    ] {
        let refusal = bad_rich("until_any = [\" \"]", &format!("until_any = {until}"));
        assert!(
            matches!(motif(&refusal), Motif::RepereInvalide { .. }),
            "{until} : {refusal}"
        );
    }
    assert!(matches!(
        motif(&bad_rich("after = \"version \"", "after = \"\"")),
        Motif::RepereInvalide { .. }
    ));
    // Repères de journal : une ligne, bornée.
    let long_sign = "x".repeat(201);
    for sign in ["", "a\\nb", long_sign.as_str()] {
        assert!(
            matches!(
                motif(&bad_rich(
                    "log_contains = \"Done\"",
                    &format!("log_contains = \"{sign}\"")
                )),
                Motif::RepereInvalide { .. }
            ),
            "{}",
            sign.len()
        );
        assert!(matches!(
            motif(&bad_rich(
                "log_contains = \"Downloading\"",
                &format!("log_contains = \"{sign}\"")
            )),
            Motif::RepereInvalide { .. }
        ));
        assert!(matches!(
            motif(&bad_rich(
                "contains = \"Saved\"\ntimeout_s = 30\nrequired = true",
                &format!("contains = \"{sign}\"\ntimeout_s = 30\nrequired = true")
            )),
            Motif::RepereInvalide { .. }
        ));
        assert!(matches!(
            motif(&bad_rich(
                "loaded_contains = \"Preparing level\"",
                &format!("loaded_contains = \"{sign}\"")
            )),
            Motif::RepereInvalide { .. }
        ));
    }
    // Aucun champ de gabarit : `derive` ne connaît que préfixe, réglage, suffixe.
    unknown_field(
        &bad_fiche_plus(
            "[[env]]\nname = \"X\"\nderive = { from = \"name\", template = \"{a}{b}\" }",
        ),
        "template",
    );
    let reader = rich();
    let (after, until) = game_version_reader(&reader).expect("lecture crue : tested");
    assert_eq!(after, "version ");
    assert_eq!(until, vec![' ']);
}

// ---- I7 : une console distante n'est jamais publiée -------------------------------------------------------------------

#[test]
fn i7_a_remote_console_is_never_published_and_its_password_is_generated() {
    for publish in ["always", "optional"] {
        assert_eq!(
            *motif(&bad_rich(
                "role = \"remote_console\"\npublish = \"never\"",
                &format!("role = \"remote_console\"\npublish = \"{publish}\"")
            )),
            Motif::ConsolePubliee,
            "{publish}"
        );
    }
    assert_eq!(
        *motif(&bad_rich(
            "password = \"rcon\"",
            "password = \"admin_password\""
        )),
        Motif::ConsoleMotDePasseNonGenere,
        "un secret saisi n'est pas un secret généré"
    );
    assert_eq!(
        *motif(&bad_rich("password = \"rcon\"", "password = \"nope\"")),
        Motif::ConsoleMotDePasseNonGenere
    );
    assert!(
        matches!(
            motif(&bad_rich(
                "port = \"rcon\"\npassword",
                "port = \"game\"\npassword"
            )),
            Motif::ConsoleIncoherente { .. }
        ),
        "le port désigné n'a pas le rôle remote_console"
    );
    // Un port de console sans déclaration `[remote_console]`.
    assert!(matches!(
        motif(&bad_fiche_plus(
            "[[port]]\nid = \"rcon\"\ncontainer = 25575\nprotocol = \"tcp\"\nrole = \"remote_console\"\npublish = \"never\"\nmapping = \"nat\""
        )),
        Motif::ConsoleIncoherente { .. }
    ));
    // Le mot de passe généré est écrit quelque part (variable ou réglage imposé), sinon la console resterait ouverte sans lui.
    let no_env = replace(
        RICH_FICHE,
        "[[env]]\nname = \"RCON_PASSWORD\"\nsecret = \"rcon\"\n\n",
        "",
    );
    let no_setting = replace(
        RICH_SETTINGS,
        "imposed = { secret = \"rcon\", why = \"Généré par Hearth, jamais affiché.\" }\n",
        "",
    );
    assert!(matches!(
        motif(&bad(&no_env, &no_setting)),
        Motif::ConsoleIncoherente { .. }
    ));
}

// ---- I8 : un contrat ne s'accepte que par un geste --------------------------------------------------------------------------

#[test]
fn i8_a_contract_is_accepted_only_by_a_gesture() {
    let at = |ou: &str| Motif::ContratAccepteDAvance { ou: ou.into() };
    assert_eq!(
        *motif(&bad_rich(
            "[[env]]\nname = \"PUID\"",
            "[[env]]\nname = \"EULA\"\nvalue = \"TRUE\"\n\n[[env]]\nname = \"PUID\""
        )),
        at("[[env]] fixe")
    );
    assert_eq!(
        *motif(&bad_rich("name = \"EDITION\"", "name = \"EULA\"")),
        at("variable fixe d'une variante")
    );
    let setting = |extra: &str| {
        format!(
            "[[setting]]\nid = \"eula_setting\"\nkey = \"EULA\"\nchannel = \"env\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Contrat\"\nhelp = \"Le contrat.\"\ntype = \"bool\"\non = \"TRUE\"\noff = \"FALSE\"\n{extra}change = \"anytime\"\nsource = \"partial\"\n"
        )
    };
    for (name, extra) in [
        ("proposée", "proposed = \"TRUE\"\n"),
        (
            "imposée",
            "imposed = { value = \"TRUE\", why = \"Non.\" }\n",
        ),
        ("libre", ""),
    ] {
        assert_eq!(
            *motif(&bad(
                RICH_FICHE,
                &format!("{RICH_SETTINGS}\n{}", setting(extra))
            )),
            at("réglage"),
            "valeur {name}"
        );
    }
    // Un contrat lié à un canal d'arguments, ou à un canal inconnu, est refusé.
    assert!(matches!(
        motif(&bad_rich(
            "channel = \"env\"\nkey = \"EULA\"",
            "channel = \"args\"\nkey = \"EULA\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    // Un contrat lié à un canal fichier serait accepté puis jamais écrit : refusé au chargement.
    for file_channel in ["props", "xml"] {
        assert!(
            matches!(
                motif(&bad_rich(
                    "channel = \"env\"\nkey = \"EULA\"",
                    &format!("channel = \"{file_channel}\"\nkey = \"EULA\"")
                )),
                Motif::ChampIncoherent { .. }
            ),
            "{file_channel}"
        );
    }
    assert!(matches!(
        motif(&bad_rich(
            "channel = \"env\"\nkey = \"EULA\"",
            "channel = \"nope\"\nkey = \"EULA\""
        )),
        Motif::ReferenceInconnue { .. }
    ));
    // La demande sans le contrat est refusée par la traduction : `AGREEMENT_REQUIRED` (voir tests_translate).
}

// ---- I9 : une fiche ne peut pas épuiser l'agent ni l'écran ---------------------------------------------------------------

fn bound(refusal: &FicheRefusee, quoi: &str, limite: usize) {
    match motif(refusal) {
        Motif::BorneDepassee {
            quoi: q, limite: l, ..
        } => {
            assert!(q.contains(quoi), "{q} ne contient pas {quoi}");
            assert_eq!(*l, limite, "{quoi}");
        }
        other => panic!("{quoi} : motif {other:?}"),
    }
}

#[test]
fn i9_a_fiche_cannot_exhaust_the_agent_or_the_screen_bounds() {
    // 256 Kio + 1 pour les deux fichiers, vérifié AVANT l'analyse.
    let padding = format!("{BASE_FICHE}\n# {}\n", "x".repeat(MAX_FILES_BYTES));
    let refusal = bad(&padding, BASE_SETTINGS);
    bound(&refusal, "taille cumulée", MAX_FILES_BYTES);
    // 401 réglages.
    let mut settings = String::from("[[theme]]\nid = \"game\"\nlabel = \"Partie\"\norder = 1\n");
    for i in 0..=MAX_SETTINGS {
        settings.push_str(&format!("\n[[setting]]\nid = \"s{i}\"\nkey = \"k{i}\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"R\"\nhelp = \"A\"\ntype = \"int\"\nchange = \"anytime\"\nsource = \"partial\"\n"));
    }
    bound(&bad(BASE_FICHE, &settings), "réglages", 400);
    // 17 ports, 9 dossiers, 7 canaux.
    let ports: String = (0..16)
        .map(|i| format!("\n[[port]]\nid = \"p{i}\"\ncontainer = {}\nprotocol = \"tcp\"\nrole = \"other\"\npublish = \"never\"\nmapping = \"nat\"\n", 8000 + i))
        .collect();
    bound(&bad_fiche_plus(&ports), "ports", 16);
    let folders: String = (0..7)
        .map(|i| format!("\n[[folder]]\nid = \"f{i}\"\nrole = \"data\"\ntarget = \"/d{i}\"\nread_only = false\nbackup = false\n"))
        .collect();
    bound(&bad_fiche_plus(&folders), "dossiers", 8);
    let channels: String = (0..5)
        .map(|i| format!("\n[[channel]]\nid = \"e{i}\"\nkind = \"env\"\n"))
        .collect();
    bound(&bad_fiche_plus(&channels), "canaux", 6);
    // 7 étapes d'arrêt.
    let wait = "[[stop.sequence.step]]\nkind = \"wait\"\nseconds = 1\n\n";
    let seven = replace(
        &replace(
            BASE_FICHE,
            "world_saved = \"by_image_on_signal\"",
            "world_saved = \"by_sequence\"",
        ),
        "[resources]",
        &format!(
            "[stop.sequence]\nstatus = \"untested\"\n\n{}[resources]",
            wait.repeat(7)
        ),
    );
    bound(&bad(&seven, BASE_SETTINGS), "étapes d'arrêt", 6);
    // Séquence de plus de 20 minutes : cinq attentes de 300 secondes et l'ordre de sortie.
    let long = "[[stop.sequence.step]]\nkind = \"wait_log\"\ncontains = \"x\"\ntimeout_s = 300\nrequired = false\n\n".repeat(5);
    let twenty_one = replace(
        &replace(
            BASE_FICHE,
            "world_saved = \"by_image_on_signal\"",
            "world_saved = \"by_sequence\"",
        ),
        "[resources]",
        &format!(
            "[stop.sequence]\nstatus = \"untested\"\n\n{long}[[stop.sequence.step]]\nkind = \"stdin_line\"\nline = \"exit\"\nquits = true\n\n[resources]"
        ),
    );
    bound(
        &bad(&twenty_one, BASE_SETTINGS),
        "durée de la séquence d'arrêt",
        1200,
    );
    // Libellés et aides bornés.
    bound(
        &bad_fiche("name = \"Demo\"", &format!("name = \"{}\"", "n".repeat(61))),
        "nom",
        60,
    );
    bound(
        &bad_fiche(
            "summary = \"Une fiche de démonstration.\"",
            &format!("summary = \"{}\"", "s".repeat(161)),
        ),
        "résumé",
        160,
    );
    bound(
        &bad_settings(
            "label = \"Nom\"",
            &format!("label = \"{}\"", "l".repeat(81)),
        ),
        "libellé",
        80,
    );
    bound(
        &bad_settings(
            "help = \"Le nom du serveur.\"",
            &format!("help = \"{}\"", "h".repeat(301)),
        ),
        "aide",
        300,
    );
    // Des réglages `text` démesurés sont refusés à la source.
    assert!(matches!(
        motif(&bad_settings("max_len = 50", "max_len = 4097")),
        Motif::BorneDepassee { .. }
    ));
    // Le contenu d'une fiche compte aussi : un fichier qui n'est pas du texte est refusé.
    assert!(matches!(
        load(Origin::Embedded, &[b'a'; 10], &[0xff, 0xfe])
            .expect_err("refusée")
            .motif,
        Motif::Syntaxe { .. }
    ));
}

// ---- Cohérence interne -------------------------------------------------------------------------------------------------------

#[test]
fn coherence_the_settings_of_a_fiche_agree_with_each_other() {
    // Plus de huit réglages `essential`.
    let essential: String = (0..8)
        .map(|i| format!("\n[[setting]]\nid = \"e{i}\"\nkey = \"e{i}\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"essential\"\nlabel = \"E\"\nhelp = \"E.\"\ntype = \"int\"\nchange = \"anytime\"\nsource = \"partial\"\n"))
        .collect();
    assert_eq!(
        *motif(&bad_settings_plus(&essential)),
        Motif::TropDEssentiels { trouve: 9 }
    );
    // Un secret n'a ni valeur proposée ni défaut non vide.
    let secret = |extra: &str| {
        format!(
            "[[setting]]\nid = \"pw\"\nkey = \"password\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"common\"\nlabel = \"Mot de passe\"\nhelp = \"Le mot de passe.\"\ntype = \"secret\"\nsecret_kind = \"game_password\"\nmax_len = 40\n{extra}change = \"anytime\"\nsource = \"verified\"\n"
        )
    };
    assert_eq!(
        *motif(&bad_settings_plus(&secret("proposed = \"hunter2\"\n"))),
        Motif::SecretAvecValeur {
            reglage: "pw".into()
        }
    );
    assert_eq!(
        *motif(&bad_settings_plus(&secret("game_default = \"hunter2\"\n"))),
        Motif::SecretAvecValeur {
            reglage: "pw".into()
        }
    );
    load_texts(
        BASE_FICHE,
        &format!("{BASE_SETTINGS}\n{}", secret("game_default = \"\"\n")),
    )
    .expect("un défaut vide est permis");
    // `name_only` : libellé fixe et niveau avancé.
    let name_only = |label: &str, level: &str| {
        format!(
            "[[setting]]\nid = \"raw\"\nkey = \"raw\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"{level}\"\nlabel = \"{label}\"\nhelp = \"Aide.\"\ntype = \"int\"\nchange = \"anytime\"\nsource = \"name_only\"\n"
        )
    };
    assert_eq!(
        *motif(&bad_settings_plus(&name_only(
            "Un libellé libre",
            "advanced"
        ))),
        Motif::ReglageNameOnlyIncoherent {
            reglage: "raw".into()
        }
    );
    assert_eq!(
        *motif(&bad_settings_plus(&name_only(
            "Réglage du jeu, sans explication",
            "common"
        ))),
        Motif::ReglageNameOnlyIncoherent {
            reglage: "raw".into()
        }
    );
    load_texts(
        BASE_FICHE,
        &format!(
            "{BASE_SETTINGS}\n{}",
            name_only("Réglage du jeu, sans explication", "advanced")
        ),
    )
    .expect("name_only conforme");
    // Un réglage imposé n'est pas `essential`.
    assert_eq!(
        *motif(&bad_settings(
            "level = \"essential\"",
            "level = \"essential\"\nimposed = { value = \"x\", why = \"Parce que.\" }"
        )),
        Motif::ReglageImposeEssentiel {
            reglage: "name".into()
        }
    );
    // Ressource non mesurée : aucun nombre.
    assert_eq!(
        *motif(&bad_fiche(
            "cpu = { evidence = \"unmeasured\" }",
            "cpu = { min_millis = 1000, evidence = \"unmeasured\" }"
        )),
        Motif::RessourceNonMesureeAvecNombre { ressource: "cpu" }
    );
    assert_eq!(
        *motif(&bad_fiche(
            "memory = { evidence = \"unmeasured\" }",
            "memory = { recommended_bytes = 5, evidence = \"unmeasured\" }"
        )),
        Motif::RessourceNonMesureeAvecNombre {
            ressource: "memory"
        }
    );
    assert_eq!(
        *motif(&bad_fiche(
            "disk = { evidence = \"unmeasured\" }",
            "disk = { install_bytes = 5, evidence = \"unmeasured\" }"
        )),
        Motif::RessourceNonMesureeAvecNombre { ressource: "disk" }
    );
    // Identifiants et clés en double.
    assert!(matches!(
        motif(&bad_settings_plus(
            &secret("").replace("id = \"pw\"", "id = \"name\"")
        )),
        Motif::IdentifiantEnDouble { .. }
    ));
    assert_eq!(
        *motif(&bad_settings_plus(
            &secret("").replace("key = \"password\"", "key = \"name\"")
        )),
        Motif::CleEnDouble {
            canal: "config".into(),
            cle: "name".into()
        }
    );
    // Références inconnues, champs absents ou hors sujet.
    assert!(matches!(
        motif(&bad_settings(
            "theme = \"game\"\nlevel = \"essential\"",
            "theme = \"nope\"\nlevel = \"essential\""
        )),
        Motif::ReferenceInconnue { .. }
    ));
    assert!(matches!(
        motif(&bad_settings(
            "channel = \"config\"\ntheme = \"game\"\nlevel = \"essential\"",
            "channel = \"nope\"\ntheme = \"game\"\nlevel = \"essential\""
        )),
        Motif::ReferenceInconnue { .. }
    ));
    assert!(matches!(
        motif(&bad_settings("label = \"Nom\"\n", "")),
        Motif::ReglageIncomplet { champ: "label", .. }
    ));
    assert!(matches!(
        motif(&bad_settings(
            "type = \"text\"\ncharset = \"line\"",
            "type = \"text\"\ncharset = \"line\"\nmin = 3"
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_settings("charset = \"line\"\n", "")),
        Motif::ReglageIncomplet {
            champ: "charset",
            ..
        }
    ));
}

#[test]
fn coherence_proposed_values_and_arguments_are_valid_for_their_type() {
    // Valeur proposée hors des bornes, hors du jeu de caractères, hors de la liste.
    assert!(matches!(
        motif(&bad_settings("proposed = \"20\"", "proposed = \"500\"")),
        Motif::ValeurProposeeInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_settings("proposed = \"20\"", "proposed = \"vingt\"")),
        Motif::ValeurProposeeInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_settings("proposed = \"Demo\"", "proposed = \"a\\nb\"")),
        Motif::ValeurProposeeInvalide { .. }
    ));
    assert!(matches!(
        motif(&bad_settings("min = 1\nmax = 128", "min = 9\nmax = 3")),
        Motif::BorneInvalide { .. }
    ));
    // Un argument a toujours son `arg`, de la bonne forme.
    assert!(matches!(
        motif(&bad_settings(
            "arg = { flag = \"-tick\", form = \"flag_value\" }\n",
            ""
        )),
        Motif::ReglageIncomplet { champ: "arg", .. }
    ));
    assert!(matches!(
        motif(&bad_settings(
            "form = \"flag_value\"",
            "form = \"flag_if_true\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(
        matches!(
            motif(&bad_settings(
                "[[setting]]\nid = \"name\"",
                "[[setting]]\narg = { flag = \"-x\", form = \"flag_value\" }\nid = \"name\""
            )),
            Motif::ChampIncoherent { .. }
        ),
        "`arg` n'a de sens que pour un canal args"
    );
    // Un port qui annonce son numéro dit quel réglage le porte.
    assert_eq!(
        *motif(&bad_fiche("mapping = \"nat\"", "mapping = \"same_number\"")),
        Motif::ChampManquant {
            champ: "game_setting".into()
        }
    );
    assert!(matches!(
        motif(&bad_fiche(
            "mapping = \"nat\"",
            "mapping = \"nat\"\ngame_setting = \"name\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "game_setting = \"server_port\"",
            "game_setting = \"nope\""
        )),
        Motif::ReferenceInconnue { .. }
    ));
    // Variantes : exactement une par défaut ; une référence inconnue est refusée.
    assert!(matches!(
        motif(&bad_rich("default = false", "default = true")),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_rich_settings(
            "only_variants = [\"bedrock\"]",
            "only_variants = [\"nope\"]"
        )),
        Motif::ReferenceInconnue { .. }
    ));
    // Une liste de variantes vide est refusée : « toutes » s'écrit en omettant le champ (BR-FICHE-060).
    assert_eq!(
        *motif(&bad_rich_settings(
            "only_variants = [\"bedrock\"]",
            "only_variants = []"
        )),
        Motif::ListeDeVariantesVide
    );
    // Mondes et contrôles croisés.
    assert!(matches!(
        motif(&bad_rich(
            "names_setting = \"level_name\"",
            "names_setting = \"motd_x\""
        )),
        Motif::ReferenceInconnue { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "names_setting = \"level_name\"",
            "names_setting = \"max_players\""
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "setting = \"hardcore\", equals",
            "setting = \"nope\", equals"
        )),
        Motif::ReferenceInconnue { .. }
    ));
    assert!(matches!(
        motif(&bad_rich(
            "must_equal = \"hard\" }",
            "must_equal = \"hard\", max = 3 }"
        )),
        Motif::ChampIncoherent { .. }
    ));
    assert!(matches!(
        motif(&bad_rich("from_version = 1", "from_version = 3")),
        Motif::BorneInvalide { .. }
    ));
    // Une clé « non exposée » ne peut pas aussi être réglée.
    assert!(matches!(
        motif(&bad_rich("key = \"ops\"", "key = \"level-name\"")),
        Motif::CleEnDouble { .. }
    ));
    // Deux sources pour une même variable.
    assert!(matches!(
        motif(&bad_rich(
            "[[env]]\nname = \"PUID\"",
            "[[env]]\nname = \"PUID\"\nvalue = \"1\"\nowner = \"uid\"\n\n[[env]]\nname = \"PUID\""
        )),
        Motif::ChampIncoherent { .. }
    ));
}

// ---- La fiche Terraria embarquée ------------------------------------------------------------------------------------------------

#[test]
fn the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest() {
    let (fiches, refused) = load_all();
    assert!(refused.is_empty(), "{refused:?}");
    let terraria = fiches
        .iter()
        .find(|f| f.id() == "terraria")
        .expect("terraria est embarquée");
    assert_eq!(
        terraria.file.status,
        Status::Preview,
        "elle reste en preview"
    );
    assert!(
        terraria.file.image.digest.is_none(),
        "l'empreinte de l'image n'est pas vérifiée : aucune n'est inventée"
    );
    assert_eq!(EMBEDDED.len(), fiches.len());
    // Embarquée, mais jamais proposée : `offered_with(false)` ne rend que les fiches `published`.
    assert!(
        super::embedded::offered_with(false)
            .iter()
            .all(|f| f.id() != "terraria"),
        "une fiche preview n'est pas proposée"
    );
    for embedded in EMBEDDED {
        assert_eq!(
            embedded.recomputed_digest(),
            embedded.digest,
            "l'empreinte de la compilation est celle de l'exécution"
        );
        let again = embedded.load().expect("chargée");
        assert_eq!(again.digest, embedded.digest);
        assert_eq!(
            again.id(),
            embedded.dir,
            "l'identifiant est le nom du dossier"
        );
    }
    // L'empreinte couvre les deux fichiers : changer un octet de l'un d'eux change l'empreinte.
    let embedded = EMBEDDED
        .iter()
        .find(|e| e.dir == "terraria")
        .expect("terraria");
    let mut altered = embedded.settings_toml.to_vec();
    altered.push(b'\n');
    assert_ne!(
        super::loading::model::fiche_digest(embedded.fiche_toml, &altered),
        embedded.digest
    );
    // La séquence d'arrêt, l'enregistrement et le signe « prêt » ne sont pas essayés.
    use super::loading::schema::fiche::Proof;
    assert_eq!(
        terraria.file.stop.sequence.as_ref().map(|s| s.status),
        Some(Proof::Untested)
    );
    assert_eq!(
        terraria.file.save.as_ref().map(|s| s.status),
        Some(Proof::Untested)
    );
    assert_eq!(
        terraria.file.ready.as_ref().map(|r| r.status),
        Some(Proof::Untested)
    );
    assert_eq!(
        ready_sign(terraria),
        None,
        "un signe untested n'est pas cru"
    );
    // Ressources : aucune n'est mesurée, aucun nombre n'est écrit.
    let r = &terraria.file.resources;
    assert!(
        r.cpu.evidence.is_unmeasured()
            && r.memory.evidence.is_unmeasured()
            && r.disk.evidence.is_unmeasured()
    );
    assert!(
        r.memory.min_bytes.is_none()
            && r.memory.recommended_bytes.is_none()
            && r.cpu.min_millis.is_none()
            && r.disk.install_bytes.is_none()
    );
}

#[test]
fn the_terraria_catalog_speaks_to_the_user_in_vous_and_every_text_stays_within_its_bounds() {
    let (fiches, _) = load_all();
    let terraria = fiches
        .iter()
        .find(|f| f.id() == "terraria")
        .expect("terraria");
    // Copie de Hearth : vouvoiement, jamais le tutoiement ni le tiret cadratin.
    let forbidden = ["tu", "te", "toi", "ton", "ta", "tes", "tien", "tiens"];
    for setting in &terraria.settings {
        for (what, text) in [("libellé", &setting.label), ("aide", &setting.help)] {
            assert!(!text.is_empty(), "{} : {what} vide", setting.id);
            assert!(
                !text.contains('\u{2014}'),
                "{} : tiret cadratin dans {what}",
                setting.id
            );
            for word in text.split(|c: char| !c.is_alphabetic()) {
                assert!(
                    !forbidden.contains(&word.to_lowercase().as_str()),
                    "{} : « {word} » (tutoiement) dans {what} : {text}",
                    setting.id
                );
            }
        }
        assert!(
            setting.label.chars().count() <= 80,
            "{} : libellé",
            setting.id
        );
        assert!(setting.help.chars().count() <= 300, "{} : aide", setting.id);
    }
    // Les 31 réglages du fichier de l'inventaire sont classés : 30 exposés (dont 4 imposés), `banlist` écarté (son
    // emplacement dans le conteneur n'est pas vérifié) ; deux arguments réglables, trois écartés.
    let in_file = terraria
        .settings
        .iter()
        .filter(|s| s.channel == "config")
        .count();
    assert_eq!(in_file, 30);
    assert!(
        terraria
            .file
            .not_exposed
            .iter()
            .all(|n| n.channel == "args" || n.key == "banlist")
    );
    assert_eq!(
        terraria.file.version, 2,
        "le contenu a changé depuis la version 1"
    );
    // Un entier dit ses bornes quand l'inventaire les donne ; un réglage à valeurs fermées dit chacune.
    let max_players = terraria.setting("max_players").expect("max_players");
    assert!(matches!(
        &max_players.spec,
        super::loading::model::ValueSpec::Int {
            min: Some(1),
            max: Some(255)
        }
    ));
    for id in [
        "priority",
        "difficulty",
        "world_size",
        "language",
        "journey_godmode",
    ] {
        let setting = terraria.setting(id).expect(id);
        assert!(
            matches!(
                &setting.spec,
                super::loading::model::ValueSpec::Choice { choices } if choices.len() >= 3
            ),
            "{id}"
        );
    }
    // Aucune version d'existence inventée : l'inventaire n'en donne aucune.
    assert!(terraria.settings.iter().all(|s| s.versions.is_none()));
}

#[test]
fn the_terraria_fiche_classes_each_of_the_36_settings_of_the_inventory() {
    let (fiches, _) = load_all();
    let terraria = fiches
        .iter()
        .find(|f| f.id() == "terraria")
        .expect("terraria");
    // Les 36 réglages du jeu (T01 à T36) : 31 du fichier, 5 seulement en argument.
    let file_keys = [
        "world",
        "worldpath",
        "worldname",
        "autocreate",
        "seed",
        "difficulty",
        "maxplayers",
        "port",
        "password",
        "motd",
        "banlist",
        "secure",
        "language",
        "upnp",
        "npcstream",
        "priority",
        "journeypermission_time_setfrozen",
        "journeypermission_time_setdawn",
        "journeypermission_time_setnoon",
        "journeypermission_time_setdusk",
        "journeypermission_time_setmidnight",
        "journeypermission_godmode",
        "journeypermission_wind_setstrength",
        "journeypermission_rain_setstrength",
        "journeypermission_time_setspeed",
        "journeypermission_rain_setfrozen",
        "journeypermission_wind_setfrozen",
        "journeypermission_increaseplacementrange",
        "journeypermission_setdifficulty",
        "journeypermission_biomespread_setfrozen",
        "journeypermission_setspawnrate",
    ];
    let arg_keys = [
        "-ip",
        "-steam",
        "-lobby",
        "-disableannouncementbox",
        "-announcementboxrange",
    ];
    assert_eq!(file_keys.len() + arg_keys.len(), 36);
    let (mut adjustable, mut imposed, mut dropped) = (0, 0, 0);
    for (channel, keys) in [("config", &file_keys[..]), ("args", &arg_keys[..])] {
        for key in keys {
            let setting = terraria
                .settings
                .iter()
                .find(|s| s.channel == channel && s.key == *key);
            let skipped = terraria
                .file
                .not_exposed
                .iter()
                .find(|n| n.channel == channel && n.key == *key);
            match (setting, skipped) {
                (Some(s), None) => {
                    if let Some(imposed_by) = &s.imposed {
                        assert!(
                            !imposed_by.why.is_empty(),
                            "{key} : un réglage imposé dit pourquoi"
                        );
                        imposed += 1;
                    } else {
                        adjustable += 1;
                    }
                }
                (None, Some(n)) => {
                    assert!(!n.why.is_empty(), "{key} : un réglage écarté dit pourquoi");
                    dropped += 1;
                }
                (None, None) => {
                    panic!("{key} ({channel}) n'est pas classé : il serait passé sous silence")
                }
                (Some(_), Some(_)) => panic!("{key} est à la fois réglable et écarté"),
            }
        }
    }
    assert_eq!(
        (adjustable, imposed, dropped),
        (28, 4, 4),
        "réglables, imposés, écartés"
    );
    assert_eq!(adjustable + imposed + dropped, 36);
    // Six questions à l'installation au plus huit.
    let essential = terraria
        .settings
        .iter()
        .filter(|s| s.level == super::loading::schema::settings::Level::Essential)
        .count();
    assert_eq!(essential, 6);
    // « Non écrit » : un défaut que l'inventaire n'a pas vérifié n'est pas inventé.
    for id in [
        "seed",
        "journey_godmode",
        "journey_time_freeze",
        "announcement_box_off",
        "announcement_box_range",
    ] {
        let setting = terraria.setting(id).expect(id);
        assert_eq!(
            setting.proposed, None,
            "{id} : rien n'est proposé, rien n'est écrit"
        );
        assert_eq!(
            setting.game_default, None,
            "{id} : défaut du jeu non vérifié"
        );
    }
    // Les 15 permissions du mode Voyage sont exposées.
    assert_eq!(
        terraria
            .settings
            .iter()
            .filter(|s| s.theme == "journey")
            .count(),
        15
    );
}

// ---- ids.lock ---------------------------------------------------------------------------------------------------------------------------

const TERRARIA_LOCK: &[u8] = include_bytes!("../../../catalog/terraria/ids.lock");

fn terraria() -> super::loading::model::Fiche {
    let embedded = EMBEDDED
        .iter()
        .find(|e| e.dir == "terraria")
        .expect("terraria");
    embedded.load().expect("chargée")
}

#[test]
fn ids_lock_the_real_lock_of_terraria_is_complete() {
    let lock = ids_lock::parse(TERRARIA_LOCK).expect("ids.lock lisible");
    let violations = ids_lock::check(&lock, &terraria());
    assert!(violations.is_empty(), "{violations:?}");
    assert_eq!(lock.ids.len(), terraria().settings.len());
}

#[test]
fn ids_lock_removing_a_setting_without_declaring_it_retired_fails() {
    let lock = ids_lock::parse(TERRARIA_LOCK).expect("lisible");
    // On retire « seed » de la fiche sans l'écrire en [[retired]].
    let settings = include_str!("../../../catalog/terraria/settings.toml");
    let start = settings.find("[[setting]]\nid = \"seed\"").expect("seed");
    let end = settings[start + 5..]
        .find("[[setting]]")
        .map(|i| start + 5 + i)
        .expect("suivant");
    let without = format!("{}{}", &settings[..start], &settings[end..]);
    let fiche = load_texts(
        include_str!("../../../catalog/terraria/fiche.toml"),
        &without,
    )
    .err();
    // « seed » est cité par `carried_by_world` : la fiche est déjà refusée (référence inconnue).
    assert!(matches!(
        fiche.map(|f| f.motif),
        Some(Motif::ReferenceInconnue { .. })
    ));
    // Avec un autre réglage (« motd »), la fiche reste valide mais `ids.lock` proteste.
    let start = settings.find("[[setting]]\nid = \"motd\"").expect("motd");
    let end = settings[start + 5..]
        .find("[[setting]]")
        .map(|i| start + 5 + i)
        .expect("suivant");
    let without = format!("{}{}", &settings[..start], &settings[end..]);
    let fiche = load_texts(
        include_str!("../../../catalog/terraria/fiche.toml"),
        &without,
    )
    .expect("encore valide");
    let violations = ids_lock::check(&lock, &fiche);
    assert_eq!(
        violations,
        vec![Violation::Disappeared { id: "motd".into() }]
    );
    assert!(violations[0].to_string().contains("[[retired]]"));
    // Écrit en [[retired]] mais le verrou le dit encore actif : il faut le mettre à jour.
    let retired = format!(
        "{without}\n[[retired]]\nid = \"motd\"\nkey = \"motd\"\nchannel = \"config\"\nlabel = \"Message\"\nuntil = \"2.0\"\nwhy = \"Retiré.\"\n"
    );
    let fiche = load_texts(
        include_str!("../../../catalog/terraria/fiche.toml"),
        &retired,
    )
    .expect("valide");
    assert_eq!(
        ids_lock::check(&lock, &fiche),
        vec![Violation::LockOutdated { id: "motd".into() }]
    );
    // Verrou à jour : plus rien à reprocher.
    let updated = String::from_utf8(TERRARIA_LOCK.to_vec())
        .expect("utf8")
        .replace(
            "{ id = \"motd\", key = \"motd\", channel = \"config\", state = \"active\", since = 1 }",
            "{ id = \"motd\", key = \"motd\", channel = \"config\", state = \"retired\", since = 1, retired_in = 1 }",
        );
    let lock = ids_lock::parse(updated.as_bytes()).expect("lisible");
    assert_eq!(ids_lock::check(&lock, &fiche), Vec::new());
    // Un identifiant retiré qui revient en [[setting]] est refusé.
    assert_eq!(
        ids_lock::check(&lock, &terraria()),
        vec![Violation::Reused { id: "motd".into() }]
    );
}

#[test]
fn ids_lock_a_changed_key_needs_a_declared_rename_and_a_new_setting_needs_a_line() {
    let lock = ids_lock::parse(TERRARIA_LOCK).expect("lisible");
    let fiche_text = include_str!("../../../catalog/terraria/fiche.toml");
    let settings = include_str!("../../../catalog/terraria/settings.toml");
    let renamed = settings.replacen("key = \"motd\"", "key = \"message\"", 1);
    let fiche = load_texts(fiche_text, &renamed).expect("valide");
    assert_eq!(
        ids_lock::check(&lock, &fiche),
        vec![Violation::KeyChanged {
            id: "motd".into(),
            was: "motd".into(),
            now: "message".into()
        }]
    );
    // Avec un `rename` déclaré dans un [[migration]], le changement de clé est permis.
    let with_rename = format!("{fiche_text}\n[[migration]]\nfrom_version = 1\nops = [{{ rename = [\"motd\", \"message\"] }}]\n")
        .replace("version = 1\nfamily", "version = 2\nfamily");
    let fiche = load_texts(&with_rename, &renamed).expect("valide");
    let violations = ids_lock::check(&lock, &fiche);
    assert!(
        !violations
            .iter()
            .any(|v| matches!(v, Violation::KeyChanged { .. })),
        "{violations:?}"
    );
    // Un nouveau réglage absent du verrou : la ligne à ajouter est donnée.
    let added = format!(
        "{settings}\n[[setting]]\nid = \"extra\"\nkey = \"extra\"\nchannel = \"config\"\ntheme = \"game\"\nlevel = \"advanced\"\nlabel = \"Extra\"\nhelp = \"Extra.\"\ntype = \"int\"\nchange = \"anytime\"\nsource = \"partial\"\n"
    );
    let fiche = load_texts(fiche_text, &added).expect("valide");
    let violations = ids_lock::check(&lock, &fiche);
    assert_eq!(violations.len(), 1);
    assert!(
        matches!(&violations[0], Violation::NotLocked { id, line } if id == "extra" && line.contains("state = \"active\""))
    );
    // Un canal changé, un identifiant en double, un verrou incohérent.
    let bad_lock = ids_lock::parse(b"ids = [\n { id = \"a\", key = \"a\", channel = \"c\", state = \"retired\", since = 1 },\n { id = \"a\", key = \"a\", channel = \"c\", state = \"active\", since = 1 },\n { id = \"b\", key = \"b\", channel = \"c\", state = \"active\", since = 9, retired_in = 2 },\n]\n").expect("lisible");
    let violations = ids_lock::check(&bad_lock, &terraria());
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, Violation::BadLock { id, .. } if id == "a"))
    );
    assert!(
        violations
            .iter()
            .any(|v| matches!(v, Violation::BadLock { id, .. } if id == "b"))
    );
    // ids.lock est fermé comme le reste.
    assert!(ids_lock::parse(b"ids = [{ id = \"a\", key = \"a\", channel = \"c\", state = \"active\", since = 1, extra = 1 }]").is_err());
}
