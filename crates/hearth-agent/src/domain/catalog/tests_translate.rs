//! Traduction fiche vers description de service : invariants I1 (une seule voie), I2 (dossiers gérés), I3
//! (arguments en jetons), I7 (console jamais publiée), I8 (contrat) et test de propriété à graine fixe.

use std::collections::{BTreeMap, BTreeSet};

use super::embedded::EMBEDDED;
use super::formats::format_for;
use super::loading::model::{Fiche, Setting, ValueSpec};
use super::loading::schema::fiche::{ChannelKind, PortRole, Publish};
use super::loading::schema::settings::ItemCharset;
use super::loading::value::{ChannelPath, Charset, container_target_ok, first_refused_char};
use super::settings_state::{InstallContext, Reason, resolve_install};
use super::test_support::*;
use super::translate::{
    DeploymentValidator, EnvDraft, EnvValue, NotYetWired, Request, ServiceSpecDraft,
    TranslateError, translate,
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

fn text_of(draft: &ServiceSpecDraft, channel: &str) -> String {
    draft
        .files
        .iter()
        .find(|f| f.channel == channel)
        .map(|f| f.content.expose().to_owned())
        .unwrap_or_default()
}

struct Inputs {
    answers: BTreeMap<String, String>,
    agreements: BTreeSet<String>,
    host_ports: BTreeMap<String, u16>,
    optional: BTreeSet<String>,
    generated: BTreeMap<String, Secret>,
}

impl Inputs {
    fn new() -> Self {
        Self {
            answers: BTreeMap::new(),
            agreements: BTreeSet::new(),
            host_ports: BTreeMap::new(),
            optional: BTreeSet::new(),
            generated: BTreeMap::new(),
        }
    }

    fn answer(mut self, id: &str, value: &str) -> Self {
        self.answers.insert(id.to_owned(), value.to_owned());
        self
    }

    fn accept(mut self, id: &str) -> Self {
        self.agreements.insert(id.to_owned());
        self
    }

    fn generated(mut self, id: &str, value: &str) -> Self {
        self.generated.insert(id.to_owned(), Secret::from(value));
        self
    }

    fn request<'a>(&'a self, variant: Option<&'a str>, world_imported: bool) -> Request<'a> {
        Request {
            variant,
            answers: &self.answers,
            agreements: &self.agreements,
            host_ports: &self.host_ports,
            optional_ports: &self.optional,
            generated: &self.generated,
            world_imported,
        }
    }
}

const RCON: &str = "Abcdefghijklmnopqrstuvwxyz012345";

// ---- Terraria ------------------------------------------------------------------------------------------------------------------

#[test]
fn terraria_with_the_proposed_values_writes_only_what_is_known_and_sends_no_order() {
    let fiche = terraria();
    let inputs = Inputs::new();
    let draft = translate(&fiche, &inputs.request(None, false)).expect("valide");
    assert_eq!(draft.fiche_id, "terraria");
    assert_eq!(draft.fiche_digest, fiche.digest_hex());
    assert_eq!(
        (draft.image_repository.as_str(), draft.image_tag.as_str()),
        ("ryshe/terraria", "vanilla-1.4.5.8")
    );
    assert_eq!(draft.image_digest, None, "aucune empreinte n'est inventée");
    assert_eq!(draft.ports.len(), 1);
    assert_eq!(
        (draft.ports[0].container, draft.ports[0].host),
        (7777, 7777)
    );
    assert!(
        draft.env.is_empty(),
        "monde nouveau : WORLD_FILENAME n'est pas définie"
    );
    assert!(draft.command.is_empty(), "aucun argument n'est proposé");
    assert_eq!(draft.files.len(), 1);
    assert_eq!(draft.files[0].path, "serverconfig.txt");
    assert_eq!(draft.files[0].folder, "config");
    assert_eq!(draft.files[0].mode.octal(), 0o600);
    assert!(!draft.files[0].holds_secret);
    assert_eq!(
        text_of(&draft, "config"),
        "worldname=Monde\nmaxplayers=8\nautocreate=3\ndifficulty=2\nworld=/root/.local/share/Terraria/Worlds/Monde.wld\nworldpath=/root/.local/share/Terraria/Worlds/\nport=7777\nupnp=0\nsecure=1\n",
        "ni password, ni motd, ni language, ni les permissions du mode Voyage : rien d'inventé"
    );
    assert!(
        !draft.stdin_open,
        "une séquence non essayée n'ouvre pas l'entrée standard"
    );
    assert!(draft.sealed.is_empty());
}

#[test]
fn terraria_answers_arguments_secrets_and_a_taken_over_world_are_translated() {
    let fiche = terraria();
    let inputs = Inputs::new()
        .answer("world_name", "Foret")
        .answer("password", "pw-pw-pw-12")
        .answer("announcement_box_off", "1")
        .answer("announcement_box_range", "-1");
    let draft = translate(&fiche, &inputs.request(None, false)).expect("valide");
    assert_eq!(
        draft.command,
        vec!["-disableannouncementbox", "-announcementboxrange", "-1"],
        "des jetons séparés, jamais une ligne"
    );
    let file = text_of(&draft, "config");
    assert!(file.contains("password=pw-pw-pw-12\n"));
    assert!(draft.files[0].holds_secret);
    assert_eq!(draft.sealed, vec!["setting:password".to_owned()]);
    assert!(
        !format!("{draft:?}").contains("pw-pw-pw-12"),
        "un secret n'apparaît dans aucun Debug"
    );
    assert!(
        draft.command.iter().all(|t| !t.contains("pw-pw")),
        "jamais un secret en argument"
    );
    // Un drapeau à « 0 » n'est pas écrit.
    let off = Inputs::new().answer("announcement_box_off", "0");
    assert!(
        translate(&fiche, &off.request(None, false))
            .expect("valide")
            .command
            .is_empty()
    );
    // Monde repris : WORLD_FILENAME est définie, les réglages figés ne sont pas écrits.
    let draft = translate(
        &fiche,
        &Inputs::new()
            .answer("world_name", "Monde")
            .request(None, true),
    )
    .expect("valide");
    assert_eq!(draft.env.len(), 1);
    assert_eq!(
        (draft.env[0].name.as_str(), draft.env[0].value.expose()),
        ("WORLD_FILENAME", "Monde.wld")
    );
    let file = text_of(&draft, "config");
    for absent in ["autocreate", "difficulty", "seed"] {
        assert!(
            !file.contains(absent),
            "{absent} est porté par le monde repris"
        );
    }
    assert!(file.contains("worldname=Monde\n"));
    // Une valeur d'argument hostile est refusée avec le caractère, jamais la valeur.
    let hostile = Inputs::new().answer("announcement_box_range", "100;rm");
    let refusal = translate(&fiche, &hostile.request(None, false)).expect_err("refusée");
    let TranslateError::Setting(refusal) = refusal else {
        panic!("{refusal:?}")
    };
    assert_eq!(
        (refusal.reason, refusal.character),
        (Reason::Bounds, None),
        "un entier est canonique"
    );
}

// ---- Fiche riche ------------------------------------------------------------------------------------------------------------------

#[test]
fn a_contract_is_required_and_accepted_only_by_the_gesture() {
    let fiche = rich();
    let none = Inputs::new().generated("rcon", RCON);
    assert_eq!(
        translate(&fiche, &none.request(None, false)).expect_err("refusée"),
        TranslateError::AgreementRequired(vec!["eula".into()])
    );
    let accepted = Inputs::new().generated("rcon", RCON).accept("eula");
    let draft = translate(&fiche, &accepted.request(None, false)).expect("acceptée");
    let eula: Vec<_> = draft.env.iter().filter(|e| e.name == "EULA").collect();
    assert_eq!(
        eula.len(),
        1,
        "une seule source pour la valeur d'acceptation"
    );
    assert_eq!(eula[0].value.expose(), "TRUE");
    assert_eq!(draft.agreements, vec!["eula".to_owned()]);
    // Aucun chemin « installer avec les valeurs proposées » n'accepte quoi que ce soit : sans le geste, rien.
    let unrelated = Inputs::new().generated("rcon", RCON).accept("other");
    assert!(matches!(
        translate(&fiche, &unrelated.request(None, false)),
        Err(TranslateError::AgreementRequired(_))
    ));
}

#[test]
fn the_rich_fiche_translates_ports_secrets_files_variants_and_never_publishes_the_console() {
    let fiche = rich();
    let mut inputs = Inputs::new()
        .generated("rcon", RCON)
        .accept("eula")
        .answer("admin_password", "adm1n-pass");
    inputs.host_ports.insert("game".into(), 26000);
    let draft = translate(&fiche, &inputs.request(None, false)).expect("valide");
    // Ports : le jeu annonce son numéro (même nombre partout) ; la requête n'est pas publiée ; la console jamais.
    assert_eq!(draft.ports.len(), 1);
    assert_eq!(
        (
            draft.ports[0].container,
            draft.ports[0].host,
            draft.ports[0].id.as_str()
        ),
        (26000, 26000, "game")
    );
    assert!(
        draft
            .ports
            .iter()
            .all(|p| p.role != PortRole::RemoteConsole)
    );
    let props = text_of(&draft, "props");
    assert!(props.contains("server-port=26000\n"), "{props}");
    assert!(
        props.contains("rcon.port=25575\n"),
        "le port interne de la console, jamais publié"
    );
    assert!(props.contains("enable-rcon=true\n"));
    assert!(props.contains(&format!("rcon.password={RCON}\n")));
    assert!(props.contains("admin-password=adm1n-pass\n"));
    assert!(
        draft
            .files
            .iter()
            .find(|f| f.channel == "props")
            .is_some_and(|f| f.holds_secret)
    );
    // Le fichier XML est créé et relisible.
    let xml = text_of(&draft, "xml");
    let channel = fiche.channel("xml").expect("xml");
    let doc = format_for(channel)
        .expect("format")
        .parse(xml.as_bytes())
        .expect("relisible");
    assert_eq!(doc.get("Difficulty"), Some("normal"));
    // Variables : propriétaire, secret généré, contrat, réglages du canal env, variante par défaut.
    let env = |name: &str| {
        draft
            .env
            .iter()
            .find(|e| e.name == name)
            .map(|e| e.value.expose().to_owned())
    };
    assert_eq!(env("PUID").as_deref(), Some("1001"));
    assert_eq!(env("PGID").as_deref(), Some("1002"));
    assert_eq!(env("RCON_PASSWORD").as_deref(), Some(RCON));
    assert_eq!(env("EULA").as_deref(), Some("TRUE"));
    assert_eq!(env("MOTD").as_deref(), Some("Bienvenue"));
    assert_eq!(env("MAX_PLAYERS").as_deref(), Some("10"));
    assert_eq!(env("HARDCORE").as_deref(), Some("FALSE"));
    assert_eq!(env("EDITION"), None);
    assert_eq!(env("BEDROCK_FLAG"), None, "réglage d'une autre variante");
    assert_eq!(draft.variant.as_deref(), Some("java"));
    assert_eq!(draft.image_tag, "2.0");
    assert_eq!(draft.command, vec!["-view", "10"]);
    assert!(draft.stdin_open, "la séquence essayée envoie des ordres");
    assert_eq!(
        draft.sealed,
        vec!["gen:rcon".to_owned(), "setting:admin_password".to_owned()]
    );
    // Aucun secret dans un Debug, aucun dans la commande.
    let shown = format!("{draft:?}");
    assert!(!shown.contains(RCON) && !shown.contains("adm1n-pass"));
    assert!(draft.command.iter().all(|t| !t.contains(RCON)));
    // Port facultatif : publié seulement sur demande.
    inputs.optional.insert("query".into());
    let draft = translate(&fiche, &inputs.request(None, false)).expect("valide");
    assert_eq!(draft.ports.len(), 2);
    assert!(
        draft
            .ports
            .iter()
            .any(|p| p.id == "query" && p.container == 25566 && p.host == 25566)
    );
    // Variante Bedrock : autre étiquette (même dépôt), variable fixe de la variante, réglage propre.
    let draft = translate(&fiche, &inputs.request(Some("bedrock"), false)).expect("valide");
    assert_eq!(draft.image_repository, "example/rich");
    assert_eq!(draft.image_tag, "bedrock-2.0");
    assert_eq!(
        draft
            .env
            .iter()
            .find(|e| e.name == "EDITION")
            .map(|e| e.value.expose()),
        Some("bedrock")
    );
    assert_eq!(
        draft
            .env
            .iter()
            .find(|e| e.name == "BEDROCK_FLAG")
            .map(|e| e.value.expose()),
        Some("0")
    );
}

#[test]
fn a_request_that_the_fiche_cannot_honour_is_refused_with_a_typed_error() {
    let fiche = rich();
    let ok = || Inputs::new().generated("rcon", RCON).accept("eula");
    let err = |inputs: &Inputs, variant: Option<&str>| {
        translate(&fiche, &inputs.request(variant, false)).expect_err("refusée")
    };
    assert_eq!(
        err(&ok(), Some("nope")),
        TranslateError::UnknownVariant("nope".into())
    );
    assert_eq!(
        err(&Inputs::new().accept("eula"), None),
        TranslateError::GeneratedSecretMissing("rcon".into())
    );
    assert_eq!(
        err(
            &Inputs::new().accept("eula").generated("rcon", "short"),
            None
        ),
        TranslateError::GeneratedSecretInvalid("rcon".into())
    );
    assert_eq!(
        err(
            &Inputs::new()
                .accept("eula")
                .generated("rcon", "Abcdefghijklmnopqrstuvwxyz01234!"),
            None
        ),
        TranslateError::GeneratedSecretInvalid("rcon".into())
    );
    let mut low = ok();
    low.host_ports.insert("game".into(), 80);
    assert_eq!(
        err(&low, None),
        TranslateError::PortRefused { id: "game".into() }
    );
    let mut same = ok();
    same.optional.insert("query".into());
    same.host_ports.insert("game".into(), 25566);
    same.host_ports.insert("query".into(), 25566);
    // tcp et udp : pas de conflit.
    translate(&fiche, &same.request(None, false)).expect("deux protocoles");
    let imposed = ok().answer("server_port", "1234");
    let TranslateError::Setting(refusal) = err(&imposed, None) else {
        panic!("attendu un refus de réglage")
    };
    assert_eq!(refusal.reason, Reason::Imposed);
    // Contrôle croisé : hardcore exige la difficulté difficile.
    let hardcore = ok().answer("hardcore", "TRUE");
    let TranslateError::Setting(refusal) = err(&hardcore, None) else {
        panic!("attendu un refus de réglage")
    };
    assert_eq!(refusal.reason, Reason::Check(0));
    translate(
        &fiche,
        &ok()
            .answer("hardcore", "TRUE")
            .answer("difficulty", "hard")
            .request(None, false),
    )
    .expect("respecté");
    // Deux sources pour une même variable.
    let clash = replace(RICH_SETTINGS, "key = \"MOTD\"", "key = \"PUID\"");
    let clash = load_texts(RICH_FICHE, &clash).expect("valide");
    assert_eq!(
        translate(&clash, &ok().request(None, false)).expect_err("conflit"),
        TranslateError::EnvConflict("PUID".into())
    );
    // Une plate-forme sans serveur Linux est listée, jamais installable.
    let ark = load_texts(
        &replace(
            RICH_FICHE,
            "platform = \"linux\"",
            "platform = { unavailable = \"Aucun serveur Linux.\" }",
        ),
        RICH_SETTINGS,
    )
    .expect("valide");
    assert_eq!(
        translate(&ark, &ok().request(None, false)).expect_err("indisponible"),
        TranslateError::PlatformUnavailable
    );
}

// ---- Propriété : toute fiche acceptée et toute réponse valide donnent une description fermée --------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }
}

fn word(rng: &mut Rng, pool: &str, min: usize, max: usize) -> String {
    let chars: Vec<char> = pool.chars().collect();
    let len = (min.max(1) + rng.below(6)).min(max);
    (0..len).map(|_| chars[rng.below(chars.len())]).collect()
}

/// Une valeur valide pour le type du réglage (jamais plus longue que 12 caractères, sans blanc de bord).
fn random_value(rng: &mut Rng, setting: &Setting) -> String {
    match &setting.spec {
        ValueSpec::Text {
            charset,
            min_len,
            max_len,
        } => match charset {
            Charset::Line => {
                let inner = word(
                    rng,
                    "abXY09 =#&<>\"'é€;.-",
                    *min_len,
                    max_len.saturating_sub(2),
                );
                format!("a{}b", inner.trim_matches(' '))
            }
            Charset::FileStem => word(rng, "abcXYZ019_-", *min_len, *max_len),
            Charset::ArgSafe => format!(
                "a{}",
                word(
                    rng,
                    "abcXYZ019_-.:",
                    min_len.saturating_sub(1),
                    max_len.saturating_sub(1)
                )
            ),
        },
        ValueSpec::Secret {
            min_len, max_len, ..
        } => word(rng, "abcXYZ0189", (*min_len).max(8), (*max_len).min(40)),
        ValueSpec::Int { min, max } => {
            let lo = min.unwrap_or(0);
            let hi = max.unwrap_or(lo + 1000);
            (lo + rng.below((hi - lo + 1) as usize) as i64).to_string()
        }
        ValueSpec::Bool { on, off } => {
            if rng.chance(50) {
                on.clone()
            } else {
                off.clone()
            }
        }
        ValueSpec::Choice { choices } => choices[rng.below(choices.len())].value.clone(),
        ValueSpec::IdList {
            item_charset,
            separator,
            max_items,
        } => {
            let count = rng.below((*max_items).min(4) + 1);
            let items: Vec<String> = (0..count)
                .map(|_| match item_charset {
                    ItemCharset::Digits => rng.below(100_000).to_string(),
                    ItemCharset::Slug => word(rng, "abc-_019", 1, 8),
                })
                .collect();
            items.join(&separator.to_string())
        }
        ValueSpec::Opaque { max_len } => word(rng, "abc019", 1, *max_len),
    }
}

fn applies(setting: &Setting, variant: Option<&str>) -> bool {
    match (&setting.only_variants, variant) {
        (None, _) => true,
        (Some(list), Some(v)) => list.iter().any(|o| o == v),
        (Some(_), None) => false,
    }
}

fn assert_closed(
    fiche: &Fiche,
    variant: Option<&str>,
    world_imported: bool,
    inputs: &Inputs,
    draft: &ServiceSpecDraft,
    label: &str,
) {
    // I1/I7 : ports publiés ≥ 1024, jamais la console, aucun doublon.
    let mut seen = BTreeSet::new();
    for port in &draft.ports {
        assert!(
            port.container >= 1024 && port.host >= 1024,
            "{label} : port sous 1024"
        );
        assert_ne!(
            port.role,
            PortRole::RemoteConsole,
            "{label} : console publiée"
        );
        assert!(
            seen.insert((port.host, port.protocol as u8)),
            "{label} : deux ports sur le même numéro"
        );
        let declared = fiche.port(&port.id).expect("port déclaré");
        assert_ne!(declared.publish, Publish::Never, "{label}");
    }
    // I2 : dossiers gérés seulement, désignés par identifiant, cibles propres, sans doublon.
    let mut targets = BTreeSet::new();
    for folder in &draft.folders {
        assert!(
            fiche.file.folder.iter().any(|d| d.id == folder.id),
            "{label}"
        );
        assert!(
            container_target_ok(&folder.target),
            "{label} : cible {}",
            folder.target
        );
        assert!(
            targets.insert(folder.target.clone()),
            "{label} : cible en double"
        );
    }
    // I3 : jetons séparés, alphabet fermé hors des drapeaux déclarés, jamais un secret.
    let flags: BTreeSet<&str> = fiche
        .settings
        .iter()
        .filter_map(|s| s.arg.as_ref().map(|a| a.flag.as_str()))
        .collect();
    let mut secrets: Vec<String> = inputs
        .generated
        .values()
        .map(|s| s.expose().to_owned())
        .collect();
    for (id, value) in &inputs.answers {
        if fiche.setting(id).is_some_and(Setting::is_secret) {
            secrets.push(value.clone());
        }
    }
    for token in &draft.command {
        let numeric = token.parse::<i64>().is_ok_and(|n| n.to_string() == *token);
        assert!(
            flags.contains(token.as_str())
                || numeric
                || first_refused_char(Charset::ArgSafe, token).is_none(),
            "{label} : jeton hors alphabet"
        );
        assert!(
            secrets.iter().all(|s| !token.contains(s.as_str())),
            "{label} : secret en argument"
        );
    }
    // Variables sans doublon ; un secret généré n'est jamais une variable « claire » qui le ferait fuiter ailleurs.
    let names: BTreeSet<&str> = draft.env.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names.len(), draft.env.len(), "{label} : variable en double");
    // I8 : la valeur d'acceptation d'un contrat vient du geste, une seule fois.
    for agreement in &fiche.file.agreement {
        let hits = draft
            .env
            .iter()
            .filter(|e| e.name == agreement.binds.key)
            .count();
        assert_eq!(hits, 1, "{label}");
        assert!(draft.agreements.contains(&agreement.id));
        assert!(inputs.agreements.contains(&agreement.id));
    }
    // Fichiers : chemin relatif propre dans un dossier déclaré, relisibles, valeurs identiques.
    let ports: BTreeMap<String, u16> = inputs.host_ports.clone();
    let ctx = InstallContext {
        variant,
        world_imported,
        host_ports: &ports,
        generated: &inputs.generated,
    };
    let resolved = resolve_install(fiche, &ctx, &inputs.answers).expect("déjà accepté");
    for file in &draft.files {
        assert!(ChannelPath::new(&file.path).is_ok(), "{label}");
        assert!(
            draft.folders.iter().any(|d| d.id == file.folder)
                || fiche.file.folder.iter().any(|d| d.id == file.folder)
        );
        let channel = fiche.channel(&file.channel).expect("canal");
        assert_eq!(channel.kind, ChannelKind::File);
        let doc = format_for(channel)
            .expect("format")
            .parse(file.content.expose().as_bytes())
            .unwrap_or_else(|e| panic!("{label} : fichier créé illisible ({e})"));
        for r in resolved.iter().filter(|r| r.channel == file.channel) {
            match &r.value {
                Some(v) => assert_eq!(doc.get(&r.key), Some(v.expose()), "{label} : {}", r.id),
                None => assert_eq!(doc.get(&r.key), None, "{label} : {} n'est pas écrit", r.id),
            }
        }
        assert_eq!(doc.duplicate_keys(), 0, "{label}");
    }
    // L'entrée standard n'est ouverte que pour envoyer un ordre.
    assert_eq!(
        draft.stdin_open,
        draft.stop.sends_orders() || super::stop_plan::save_plan(fiche).is_some(),
        "{label}"
    );
    // Les valeurs en clair ne contiennent jamais un secret saisi.
    for env in &draft.env {
        if let EnvValue::Plain(value) = &env.value {
            assert!(
                secrets.iter().all(|s| value != s),
                "{label} : secret en clair dans {}",
                env.name
            );
        }
    }
}

#[test]
fn property_every_accepted_fiche_and_valid_answers_give_a_closed_description() {
    let fiches = [terraria(), base(), rich()];
    let mut accepted = 0usize;
    let mut check_refused = 0usize;
    for (n, fiche) in fiches.iter().enumerate() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (n as u64 + 1));
        for case in 0..150 {
            let variants: Vec<&str> = fiche.file.variant.iter().map(|v| v.id.as_str()).collect();
            let variant = if variants.is_empty() {
                None
            } else {
                Some(variants[rng.below(variants.len())])
            };
            let world_imported = rng.chance(40);
            let mut inputs = Inputs::new();
            for setting in &fiche.settings {
                let carried = world_imported
                    && fiche
                        .file
                        .worlds
                        .as_ref()
                        .is_some_and(|w| w.carried_by_world.contains(&setting.id));
                if setting.imposed.is_some()
                    || !applies(setting, variant)
                    || carried
                    || !rng.chance(50)
                {
                    continue;
                }
                inputs
                    .answers
                    .insert(setting.id.clone(), random_value(&mut rng, setting));
            }
            for (i, port) in fiche.file.port.iter().enumerate() {
                if rng.chance(50) {
                    inputs.host_ports.insert(
                        port.id.clone(),
                        20_000 + (i as u16) * 100 + rng.below(50) as u16,
                    );
                }
                if port.publish == Publish::Optional && rng.chance(50) {
                    inputs.optional.insert(port.id.clone());
                }
            }
            for secret in &fiche.file.secret {
                inputs.generated.insert(
                    secret.id.clone(),
                    Secret::new(
                        word(
                            &mut rng,
                            "abcXYZ0189",
                            secret.length as usize,
                            secret.length as usize,
                        )
                        .chars()
                        .cycle()
                        .take(secret.length as usize)
                        .collect(),
                    ),
                );
            }
            let accept_all = rng.chance(70);
            if accept_all {
                inputs.agreements = fiche.file.agreement.iter().map(|a| a.id.clone()).collect();
            }
            let label = format!("{} cas {case}", fiche.id());
            match translate(fiche, &inputs.request(variant, world_imported)) {
                Ok(draft) => {
                    accepted += 1;
                    assert!(
                        accept_all || fiche.file.agreement.is_empty(),
                        "{label} : contrat accepté d'office"
                    );
                    assert_closed(fiche, variant, world_imported, &inputs, &draft, &label);
                }
                Err(TranslateError::AgreementRequired(ids)) => {
                    assert!(!accept_all && !ids.is_empty(), "{label}");
                }
                Err(TranslateError::Setting(refusal))
                    if matches!(refusal.reason, Reason::Check(_)) =>
                {
                    check_refused += 1
                }
                Err(other) => panic!("{label} : refus inattendu {other:?}"),
            }
        }
    }
    assert!(
        accepted > 200,
        "le générateur doit produire surtout des réponses valides ({accepted})"
    );
    assert!(check_refused > 0, "les contrôles croisés sont exercés");
}

// ---- Validateur du déploiement ordinaire : à brancher ---------------------------------------------------------------------------

/// L'interface est posée (`DeploymentValidator`) ; le branchement de la traduction est HRT-101. Cette garde
/// empêche qu'on prenne le placeholder `NotYetWired` pour une validation.
#[test]
fn the_ordinary_deployment_validator_is_not_wired_yet_and_says_so() {
    let draft = translate(&terraria(), &Inputs::new().request(None, false)).expect("valide");
    let verdict = NotYetWired.validate(&draft);
    assert!(
        verdict
            .as_ref()
            .is_err_and(|why| why.contains("non branché")),
        "{verdict:?}"
    );
}

/// I1 « plan égal à celui du validateur » : toute fiche embarquée, une fois traduite, est acceptée par le validateur
/// du déploiement ordinaire (HRT-68), le même que celui d'une image déployée à la main.
#[test]
fn every_accepted_fiche_is_accepted_by_the_ordinary_deployment_validator() {
    let validator: &dyn DeploymentValidator = &crate::domain::services::deploy::OrdinaryDeployment;
    for embedded in EMBEDDED {
        let fiche = embedded.load().expect("chargée");
        let draft = translate(&fiche, &Inputs::new().request(None, false)).expect("valide");
        validator
            .validate(&draft)
            .unwrap_or_else(|why| panic!("{} : {why}", embedded.dir));
    }
}

/// Le validateur du déploiement ordinaire refuse ce que le déploiement fait à la main refuse, par un code fixe qui
/// ne cite jamais la valeur (HRT-68, I1).
#[test]
fn the_ordinary_deployment_validator_refuses_what_a_hand_made_deployment_refuses() {
    use crate::domain::services::deploy::OrdinaryDeployment;
    let draft = |change: &dyn Fn(&mut ServiceSpecDraft)| {
        let mut draft =
            translate(&terraria(), &Inputs::new().request(None, false)).expect("valide");
        change(&mut draft);
        OrdinaryDeployment.validate(&draft)
    };
    assert_eq!(draft(&|_| {}), Ok(()));
    assert_eq!(
        draft(&|d| d.image_repository = "Bad Image".into()),
        Err("image:bad_format".into())
    );
    assert_eq!(
        draft(&|d| d.env.push(EnvDraft {
            name: "1-SENTINEL".into(),
            value: EnvValue::Plain("SENTINEL-VALUE".into()),
        })),
        Err("env:bad_name".into())
    );
    assert_eq!(
        draft(&|d| d.folders.push(d.folders[0].clone())),
        Err("mounts:duplicate".into())
    );
    assert_eq!(
        draft(&|d| d.command = vec!["x".into(); 65]),
        Err("command:bad_format".into())
    );
    assert_eq!(
        draft(&|d| d.ports.push(d.ports[0].clone())),
        Err("ports:duplicate".into())
    );
}
