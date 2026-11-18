//! La fiche Minecraft Java embarquée (HRT-116) : ce que Hearth passe à l'image `itzg/minecraft-server`, par la même
//! traduction que toute fiche. Le scénario qui compte : un pack serveur Fabric 1.20.1 (Prominence II) avec une grosse
//! mémoire, un contrat accepté par un geste, un arrêt par le signal de l'image, aucune voie CurseForge.

use std::collections::{BTreeMap, BTreeSet};

use super::embedded::load_all;
use super::loading::model::Fiche;
use super::loading::schema::fiche::{PackGenre, WorldSaved};
use super::translate::{Request, ServiceSpecDraft, TranslateError, translate};
use crate::domain::secret::Secret;

fn minecraft() -> Fiche {
    let (fiches, refused) = load_all();
    assert!(refused.is_empty(), "aucune fiche refusée");
    fiches
        .into_iter()
        .find(|f| f.id() == "minecraft")
        .expect("la fiche minecraft est embarquée")
}

struct Ask {
    variant: Option<&'static str>,
    answers: BTreeMap<String, String>,
    accept: bool,
    pack: Option<PackGenre>,
}

impl Ask {
    fn new() -> Self {
        Self {
            variant: None,
            answers: BTreeMap::new(),
            accept: true,
            pack: None,
        }
    }
    fn variant(mut self, id: &'static str) -> Self {
        self.variant = Some(id);
        self
    }
    fn answer(mut self, id: &str, value: &str) -> Self {
        self.answers.insert(id.to_owned(), value.to_owned());
        self
    }
    fn pack(mut self, genre: PackGenre) -> Self {
        self.pack = Some(genre);
        self
    }
    fn without_the_contract(mut self) -> Self {
        self.accept = false;
        self
    }

    fn translate(&self, fiche: &Fiche) -> Result<ServiceSpecDraft, TranslateError> {
        let agreements: BTreeSet<String> = if self.accept {
            BTreeSet::from(["eula".to_owned()])
        } else {
            BTreeSet::new()
        };
        let generated = BTreeMap::from([(
            "rcon".to_owned(),
            Secret::from("Abcdefghijklmnopqrstuvwxyz012345"),
        )]);
        translate(
            fiche,
            &Request {
                variant: self.variant,
                answers: &self.answers,
                agreements: &agreements,
                host_ports: &BTreeMap::new(),
                optional_ports: &BTreeSet::new(),
                generated: &generated,
                world_imported: false,
                pack: self.pack,
            },
        )
    }
}

fn env(draft: &ServiceSpecDraft, name: &str) -> Option<String> {
    draft
        .env
        .iter()
        .find(|e| e.name == name)
        .map(|e| e.value.expose().to_owned())
}

#[test]
fn the_fiche_is_published_with_a_pinned_image_per_java_a_tested_ready_sign_and_a_measured_memory() {
    let fiche = minecraft();
    let file = &fiche.file;
    // Publiée par le banc d'essai réel du 15/11/2026 (ADR-0053) : signe « prêt » essayé, mémoire mesurée.
    assert_eq!(
        file.status,
        super::loading::schema::fiche::Status::Published
    );
    assert_eq!(
        file.ready.as_ref().map(|r| r.status),
        Some(super::loading::schema::fiche::Proof::Tested)
    );
    assert_eq!(
        file.resources.memory.min_bytes,
        Some(2 * 1024 * 1024 * 1024)
    );
    assert_eq!(
        file.resources.memory.recommended_bytes,
        Some(4 * 1024 * 1024 * 1024)
    );
    assert!(super::loading::memory::recommended_gap(file).is_none());
    assert_eq!(file.image.repository, "itzg/minecraft-server");
    assert!(
        file.image
            .digest
            .as_deref()
            .is_some_and(|d| d.starts_with("sha256:"))
    );
    // Les moteurs à mods de la 1.20 tournent avec Java 17, NeoForge avec Java 21, les autres avec Java 25.
    let tag = |id: &str| {
        file.variant
            .iter()
            .find(|v| v.id == id)
            .and_then(|v| v.image_tag.clone())
    };
    assert_eq!(tag("fabric").as_deref(), Some("java17"));
    assert_eq!(tag("neoforge").as_deref(), Some("java21"));
    assert_eq!(tag("vanilla"), None);
    // Spigot n'est pas proposé (l'image dit que son téléchargement n'est plus pris en charge).
    assert!(file.variant.iter().all(|v| v.id != "spigot"));
    let ids: Vec<&str> = file.variant.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "vanilla", "paper", "purpur", "fabric", "quilt", "forge", "neoforge", "modrinth"
        ]
    );
}

#[test]
fn prominence_ii_the_server_pack_with_fabric_reaches_the_image_as_the_image_expects() {
    let fiche = minecraft();
    let draft = Ask::new()
        .variant("fabric")
        .answer("version", "1.20.1")
        .pack(PackGenre::ServerPack)
        .translate(&fiche)
        .expect("traduite");
    // Le moteur, la version, le contrat accepté par le geste, le chemin du pack serveur.
    assert_eq!(env(&draft, "TYPE").as_deref(), Some("FABRIC"));
    assert_eq!(env(&draft, "VERSION").as_deref(), Some("1.20.1"));
    assert_eq!(env(&draft, "EULA").as_deref(), Some("TRUE"));
    assert_eq!(
        env(&draft, "GENERIC_PACK").as_deref(),
        Some("/packs/pack.zip")
    );
    // Java 17 pour Fabric 1.20.1, par l'empreinte.
    assert_eq!(draft.image_tag, "java17");
    assert!(
        draft
            .image_digest
            .as_deref()
            .is_some_and(|d| d.starts_with("sha256:"))
    );
    // La mémoire : la JVM suit la limite du service (le curseur de l'installation), jamais un chiffre de la fiche.
    assert_eq!(env(&draft, "MEMORY").as_deref(), Some(""));
    assert_eq!(
        env(&draft, "JVM_XX_OPTS").as_deref(),
        Some("-XX:MaxRAMPercentage=75")
    );
    // FIX:01M7Q1M71TKSBWVV0C0GTXWWXZ : la fiche déclare la variable qui reçoit la limite de mémoire.
    assert_eq!(fiche.jvm_options_env(), Some("JVM_XX_OPTS"));
    // Arrêt propre : l'image reçoit le signal, attend l'enregistrement du monde ; le moteur attend plus longtemps.
    assert_eq!(env(&draft, "STOP_DURATION").as_deref(), Some("180"));
    assert_eq!(fiche.file.stop.world_saved, WorldSaved::ByImageOnSignal);
    assert!(fiche.file.stop.grace_s > 180);
    // Le port du jeu seulement ; la console interne n'est jamais publiée.
    assert_eq!(draft.ports.len(), 1);
    assert_eq!(
        (draft.ports[0].host, draft.ports[0].container),
        (25565, 25565)
    );
    // Les dossiers : les données inscriptibles et sauvegardées, les packs en lecture seule.
    let folder = |id: &str| draft.folders.iter().find(|f| f.id == id).expect("dossier");
    assert!(!folder("data").read_only && folder("data").backup);
    assert!(folder("packs").read_only && !folder("packs").backup);
    assert_eq!(folder("packs").target, "/packs");
    // Aucune voie CurseForge nulle part.
    assert!(
        draft.env.iter().all(|e| !e.name.starts_with("CF_")
            && !e.value.expose().eq_ignore_ascii_case("AUTO_CURSEFORGE"))
    );
    // Le mot de passe de la console interne est le secret tiré par Hearth.
    assert_eq!(
        env(&draft, "RCON_PASSWORD").as_deref(),
        Some("Abcdefghijklmnopqrstuvwxyz012345")
    );
}

#[test]
fn the_default_install_is_the_official_server_with_the_latest_version_and_no_pack() {
    let fiche = minecraft();
    let draft = Ask::new().translate(&fiche).expect("traduite");
    assert_eq!(env(&draft, "TYPE").as_deref(), Some("VANILLA"));
    assert_eq!(env(&draft, "VERSION").as_deref(), Some("LATEST"));
    assert!(env(&draft, "GENERIC_PACK").is_none() && env(&draft, "MODRINTH_MODPACK").is_none());
    assert_eq!(draft.image_tag, "java25");
}

#[test]
fn without_the_contract_gesture_nothing_is_translated_and_the_eula_is_never_a_default() {
    let fiche = minecraft();
    let refused = Ask::new()
        .without_the_contract()
        .translate(&fiche)
        .unwrap_err();
    assert_eq!(
        refused,
        TranslateError::AgreementRequired(vec!["eula".to_owned()])
    );
    // La valeur d'acceptation ne vient d'aucune variable fixe ni variante ni réglage.
    for variant in &fiche.file.variant {
        assert!(variant.env.iter().all(|e| e.name != "EULA"));
    }
    assert!(fiche.file.env.iter().all(|e| e.name != "EULA"));
    assert!(fiche.settings.iter().all(|s| s.key != "EULA"));
}

#[test]
fn a_pack_genre_only_goes_with_the_engines_the_fiche_allows_for_it() {
    let fiche = minecraft();
    // Un pack Modrinth avec Fabric : le moteur vient du pack, la variante doit être « selon le pack ».
    assert_eq!(
        Ask::new()
            .variant("fabric")
            .pack(PackGenre::ModrinthPack)
            .translate(&fiche)
            .unwrap_err(),
        TranslateError::PackVariantMismatch
    );
    let draft = Ask::new()
        .variant("modrinth")
        .pack(PackGenre::ModrinthPack)
        .translate(&fiche)
        .expect("traduite");
    assert_eq!(env(&draft, "TYPE").as_deref(), Some("MODRINTH"));
    assert_eq!(
        env(&draft, "MODRINTH_MODPACK").as_deref(),
        Some("/packs/pack.mrpack")
    );
    // Un pack serveur avec « selon le pack Modrinth » : refusé aussi.
    assert_eq!(
        Ask::new()
            .variant("modrinth")
            .pack(PackGenre::ServerPack)
            .translate(&fiche)
            .unwrap_err(),
        TranslateError::PackVariantMismatch
    );
}

#[test]
fn a_game_setting_goes_to_the_image_by_its_variable_and_a_bad_value_is_refused() {
    let fiche = minecraft();
    let draft = Ask::new()
        .answer("max_players", "8")
        .answer("difficulty", "hard")
        .answer("motd", "Bienvenue")
        .translate(&fiche)
        .expect("traduite");
    assert_eq!(env(&draft, "MAX_PLAYERS").as_deref(), Some("8"));
    assert_eq!(env(&draft, "DIFFICULTY").as_deref(), Some("hard"));
    assert_eq!(env(&draft, "MOTD").as_deref(), Some("Bienvenue"));
    for (id, value) in [
        ("max_players", "0"),
        ("max_players", "1001"),
        ("difficulty", "impossible"),
        ("version", "1.20.1; rm -rf /"),
        ("level_name", "../monde"),
    ] {
        assert!(
            matches!(
                Ask::new().answer(id, value).translate(&fiche),
                Err(TranslateError::Setting(_))
            ),
            "{id} = {value:?} doit être refusé"
        );
    }
}

#[test]
fn the_fiche_exposes_no_player_identity_and_no_curseforge_key() {
    let fiche = minecraft();
    // Aucun réglage ne nomme un compte (opérateurs, liste blanche : par les fichiers), aucun ne touche CF_*.
    for setting in &fiche.settings {
        assert!(
            !["OPS", "WHITELIST", "OP_PERMISSION_LEVEL"].contains(&setting.key.as_str()),
            "{}",
            setting.id
        );
        assert!(!setting.key.starts_with("CF_"), "{}", setting.id);
    }
    let mods = fiche.file.mods.as_ref().expect("voies de mods");
    assert_eq!(mods.ways.len(), 1, "seul le dépôt d'un pack est offert");
}
