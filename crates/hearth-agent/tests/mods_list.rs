//! La liste des mods d'un serveur installé au niveau du cas d'usage (HRT-256, ADR-0062, BR-MOD-020 à 033).
//! Vraie base, faux dossiers de données : jars fabriqués (zip sans compression), jamais un binaire versionné.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::mods::{ModsError, ModsService};
use hearth_agent::application::ports::{ModOrigins, SafeFs};
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::mods::game_info::OriginRow;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::spec::{
    SpecAnswer, SpecBody, SpecFiche, SpecImage, SpecRestart, SpecSecrets, SpecSettings,
};
use hearth_agent::infrastructure::sqlite::SqliteModOrigins;
use hearth_proto::api::mods::{
    Applicable, MinecraftFrom, ModOrigin, ModPlace, ModsView, PackKind, VerdictCode,
};
use support::fake_safe_fs::FakeSafeFs;
use support::{Env, env};

/// Un zip sans compression : un jar minimal mais valable pour le lecteur d'archives hostiles.
fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let offset = u32::try_from(out.len()).unwrap();
        let size = u32::try_from(data.len()).unwrap();
        let name_len = u16::try_from(name.len()).unwrap();
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
        out.extend_from_slice(&crc.sum().to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend_from_slice(&crc.sum().to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = u32::try_from(out.len()).unwrap();
    let central_size = u32::try_from(central.len()).unwrap();
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    let count = u16::try_from(entries.len()).unwrap();
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

fn fabric(id: &str, version: &str, minecraft: &str, depends: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"schemaVersion":1,"id":"{id}","version":"{version}","name":"{id}","depends":{{"minecraft":"{minecraft}"{depends}}}}}"#
    );
    jar(&[("fabric.mod.json", json.as_bytes())])
}

fn forge(id: &str) -> Vec<u8> {
    let toml = format!(
        "modLoader = \"javafml\"\nloaderVersion = \"[47,)\"\nlicense = \"MIT\"\n[[mods]]\nmodId = \"{id}\"\nversion = \"1.0.0\"\n"
    );
    jar(&[("META-INF/mods.toml", toml.as_bytes())])
}

struct Bench {
    env: Env,
    fs: Arc<FakeSafeFs>,
    origins: Arc<SqliteModOrigins>,
    key: ServiceKey,
}

async fn bench(n: u32, variant: &str, version: Option<&str>) -> Bench {
    let env = env().await;
    let fs = Arc::new(FakeSafeFs::new());
    let key = support::game::key_of(n);
    let catalog = CatalogService::embedded(true);
    let digest = catalog.fiche("minecraft").expect("minecraft").digest_hex();
    let mut fiche = SpecFiche::new("minecraft", 5, digest);
    fiche.variant = Some(variant.to_owned());
    if let Some(version) = version {
        fiche.answers.push(SpecAnswer {
            id: "version".into(),
            value: Some(version.to_owned()),
            secret: false,
            frozen: false,
        });
    }
    let body = SpecBody {
        name: format!("minecraft-{n}"),
        image: SpecImage {
            repository: "itzg/minecraft-server".into(),
            tag: "java21".into(),
            digest: None,
        },
        ports: vec![],
        mounts: vec![],
        restart: SpecRestart::UnlessStopped,
        stop_grace_s: Some(30),
        stdin_open: true,
        cpu_millis: Some(1000),
        memory_bytes: Some(2 << 30),
        settings: SpecSettings::default(),
        fiche: Some(fiche),
    };
    support::game::specs_of(&env)
        .save(&key, None, body, &SpecSecrets::default(), "marie")
        .await
        .expect("description");
    fs.seed_dir(&key, "data");
    let origins = Arc::new(SqliteModOrigins::new(env.db.pool().clone()));
    Bench {
        env,
        fs,
        origins,
        key,
    }
}

impl Bench {
    fn service(&self) -> ModsService {
        ModsService::new(
            support::game::specs_of(&self.env),
            Arc::new(CatalogService::embedded(true)),
            self.fs.clone(),
            self.origins.clone(),
        )
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        self.fs.seed_file(&self.key, &format!("data/{rel}"), bytes);
    }

    async fn list(&self) -> ModsView {
        self.service().list(&self.key).await.expect("liste")
    }
}

fn find<'a>(view: &'a ModsView, file: &str) -> &'a hearth_proto::api::mods::ModItem {
    view.mods
        .iter()
        .find(|item| item.file.as_deref() == Some(file))
        .unwrap_or_else(|| panic!("{file} absent de {:?}", view.mods))
}

#[tokio::test]
async fn a_fabric_server_lists_mods_with_a_verdict_state_and_the_game_version_from_the_setting() {
    let b = bench(1, "fabric", Some("1.20.1")).await;
    b.put(
        "mods/lithium.jar",
        &fabric("lithium", "0.11.2", "~1.20.1", ""),
    );
    b.put("mods/ancient.jar", &fabric("ancient", "1.0.0", "~1.16", ""));
    b.put(
        "mods/needy.jar",
        &fabric("needy", "1.0.0", "~1.20.1", r#","fabric-api":"*""#),
    );
    b.put("mods/forgey.jar", &forge("forgey"));
    b.put("mods/junk.jar", b"ceci n'est pas une archive");
    b.put(
        "mods/off.jar.disabled",
        &fabric("off", "1.0.0", "~1.20.1", ""),
    );
    b.put("mods/readme.txt", b"pas un mod");
    b.put(
        "mods-retires/old.jar",
        &fabric("old", "0.1.0", "~1.20.1", ""),
    );
    let view = b.list().await;
    assert_eq!(view.applicable, Applicable::Mods);
    assert_eq!(view.game.loader.as_deref(), Some("fabric"));
    assert_eq!(view.game.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(view.game.minecraft_from, MinecraftFrom::Setting);
    assert!(!view.game.updates_every_start);
    assert_eq!(view.game.pack, PackKind::None);
    assert_eq!(view.mods.len(), 7, "le fichier texte n'est pas un mod");
    assert_eq!(
        find(&view, "lithium.jar").verdict.code,
        VerdictCode::Compatible
    );
    assert_eq!(
        find(&view, "ancient.jar").verdict.code,
        VerdictCode::IncompatibleMinecraft
    );
    let needy = find(&view, "needy.jar");
    assert_eq!(needy.verdict.code, VerdictCode::ToComplete);
    assert_eq!(needy.verdict.missing, vec!["fabric-api".to_owned()]);
    assert_eq!(
        find(&view, "forgey.jar").verdict.code,
        VerdictCode::IncompatibleLoader
    );
    assert_eq!(
        find(&view, "junk.jar").verdict.code,
        VerdictCode::Unrecognized
    );
    assert!(find(&view, "junk.jar").identity.is_none());
    let off = find(&view, "off.jar.disabled");
    assert!(!off.enabled);
    assert_eq!(off.identity.as_ref().unwrap().id, "off");
    let old = find(&view, "old.jar");
    assert_eq!(old.place, ModPlace::Retired);
    assert_eq!(old.origin, ModOrigin::Manual);
    assert_eq!(view.unread, 0);
    assert!(!view.truncated);
}

#[tokio::test]
async fn two_versions_of_a_mod_are_a_duplicate_unless_one_is_disabled() {
    let b = bench(2, "fabric", Some("1.20.1")).await;
    b.put("mods/a-1.jar", &fabric("a", "1.0.0", "~1.20.1", ""));
    b.put("mods/a-2.jar", &fabric("a", "2.0.0", "~1.20.1", ""));
    let view = b.list().await;
    assert_eq!(find(&view, "a-1.jar").verdict.code, VerdictCode::Duplicate);
    assert_eq!(find(&view, "a-2.jar").verdict.code, VerdictCode::Duplicate);
    b.fs.seed_file(
        &b.key,
        "data/mods/a-1.jar.disabled",
        &fabric("a", "1.0.0", "~1.20.1", ""),
    );
    b.fs.open_service_root(&b.key)
        .unwrap()
        .remove_tree(&RelPath::parse("data/mods/a-1.jar").unwrap())
        .unwrap();
    let view = b.list().await;
    assert_eq!(find(&view, "a-2.jar").verdict.code, VerdictCode::Compatible);
    assert!(!find(&view, "a-1.jar.disabled").enabled);
}

#[tokio::test]
async fn a_latest_server_reads_the_version_in_the_log_and_without_a_log_the_version_is_unknown() {
    let b = bench(3, "fabric", Some("LATEST")).await;
    b.put(
        "mods/lithium.jar",
        &fabric("lithium", "0.11.2", "~1.20.1", ""),
    );
    let unknown = b.list().await;
    assert_eq!(unknown.game.minecraft, None);
    assert_eq!(unknown.game.minecraft_from, MinecraftFrom::Unknown);
    assert!(unknown.game.updates_every_start);
    assert_eq!(
        find(&unknown, "lithium.jar").verdict.code,
        VerdictCode::Unverified
    );
    b.put(
        "logs/latest.log",
        b"[main/INFO]: x\n[Server thread/INFO]: Starting minecraft server version 1.20.1\n",
    );
    let known = b.list().await;
    assert_eq!(known.game.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(known.game.minecraft_from, MinecraftFrom::Log);
    assert_eq!(
        find(&known, "lithium.jar").verdict.code,
        VerdictCode::Compatible
    );
}

#[tokio::test]
async fn a_modrinth_server_takes_its_game_and_loader_from_the_manifest_and_marks_the_pack_files() {
    let b = bench(4, "modrinth", Some("LATEST")).await;
    b.put(
        ".modrinth-modpack-manifest.json",
        br#"{"files":["mods/pack.jar"],"dependencies":{"minecraft":"1.20.1","fabric-loader":"0.15.0"}}"#,
    );
    b.put("mods/pack.jar", &fabric("pack", "1.0.0", "~1.20.1", ""));
    b.put("mods/mine.jar", &fabric("mine", "1.0.0", "~1.20.1", ""));
    let view = b.list().await;
    assert_eq!(view.game.loader.as_deref(), Some("fabric"));
    assert_eq!(view.game.minecraft.as_deref(), Some("1.20.1"));
    assert_eq!(view.game.minecraft_from, MinecraftFrom::ModrinthManifest);
    assert_eq!(view.game.pack, PackKind::ModrinthPack);
    assert_eq!(find(&view, "pack.jar").origin, ModOrigin::ModrinthPack);
    assert_eq!(find(&view, "mine.jar").origin, ModOrigin::Manual);
}

#[tokio::test]
async fn the_origin_of_a_pack_file_needs_the_same_name_size_and_crc_a_changed_file_is_manual() {
    let b = bench(5, "fabric", Some("1.20.1")).await;
    let original = fabric("pack", "1.0.0", "~1.20.1", "");
    let mut crc = flate2::Crc::new();
    crc.update(&original);
    b.put("mods/pack.jar", &original);
    b.put(
        "mods/changed.jar",
        &fabric("changed", "9.9.9", "~1.20.1", ""),
    );
    let rows = [
        OriginRow {
            file_name: "pack.jar".into(),
            size: original.len() as u64,
            crc32: Some(crc.sum()),
            sha256: None,
            from_hearth: false,
        },
        OriginRow {
            file_name: "changed.jar".into(),
            size: 3,
            crc32: Some(1),
            sha256: None,
            from_hearth: false,
        },
    ];
    b.origins
        .record_server_pack(&b.key, &rows, time::OffsetDateTime::now_utc())
        .await
        .unwrap();
    let view = b.list().await;
    assert_eq!(find(&view, "pack.jar").origin, ModOrigin::ServerPack);
    assert_eq!(find(&view, "changed.jar").origin, ModOrigin::Manual);
    assert_eq!(view.game.pack, PackKind::ServerPack);
}

#[tokio::test]
async fn links_special_files_and_unsafe_names_are_listed_by_rank_and_never_opened() {
    let b = bench(6, "fabric", Some("1.20.1")).await;
    b.put("mods/ok.jar", &fabric("ok", "1.0.0", "~1.20.1", ""));
    b.fs.seed_symlink(&b.key, "data/mods/link.jar", "/etc/passwd");
    b.fs.seed_special(&b.key, "data/mods/pipe.jar");
    b.put(
        "mods/bad\u{202e}name.jar",
        &fabric("x", "1.0.0", "~1.20.1", ""),
    );
    let view = b.list().await;
    assert_eq!(view.mods.len(), 4);
    let hidden: Vec<_> = view.mods.iter().filter(|m| !m.readable_name).collect();
    assert_eq!(hidden.len(), 1);
    assert!(hidden[0].file.is_none());
    assert_eq!(hidden[0].verdict.code, VerdictCode::Unrecognized);
    assert_eq!(
        find(&view, "link.jar").verdict.code,
        VerdictCode::Unrecognized
    );
    assert_eq!(
        find(&view, "pipe.jar").verdict.code,
        VerdictCode::Unrecognized
    );
    let opened = b.fs.opened_paths();
    assert!(
        opened.contains(&"data/mods/ok.jar".to_owned()),
        "{opened:?}"
    );
    assert!(
        opened.iter().all(|p| p.starts_with("data/mods/ok")
            || p == "data/logs/latest.log"
            || p == "packs/pack.zip"),
        "{opened:?}"
    );
}

#[tokio::test]
async fn a_mods_folder_that_is_a_link_is_blocked_and_vanilla_paper_have_no_mods_and_no_read() {
    let b = bench(7, "fabric", Some("1.20.1")).await;
    b.fs.seed_symlink(&b.key, "data/mods", "/etc");
    let blocked = b.service().list(&b.key).await.unwrap_err();
    assert!(matches!(blocked, ModsError::FolderBlocked), "{blocked:?}");
    for (n, variant, expected) in [
        (8, "vanilla", Applicable::NoMods),
        (9, "paper", Applicable::Plugins),
        (10, "purpur", Applicable::Plugins),
    ] {
        let b = bench(n, variant, Some("1.20.1")).await;
        b.put("mods/a.jar", &fabric("a", "1.0.0", "~1.20.1", ""));
        let view = b.list().await;
        assert_eq!(view.applicable, expected, "{variant}");
        assert!(view.mods.is_empty());
        assert!(b.fs.listed_paths().is_empty(), "aucune lecture de dossier");
    }
}

#[tokio::test]
async fn a_service_without_a_fiche_is_not_a_game_service() {
    let b = bench(11, "fabric", None).await;
    let other = support::game::key_of(12);
    let result = b.service().list(&other).await;
    assert!(matches!(result, Err(ModsError::NotAGameService)));
}

#[tokio::test]
async fn an_exhausted_budget_reports_the_unread_jars_and_never_a_guess() {
    let b = bench(13, "fabric", Some("1.20.1")).await;
    for n in 0..5 {
        b.put(
            &format!("mods/m{n}.jar"),
            &fabric(&format!("m{n}"), "1.0.0", "~1.20.1", ""),
        );
    }
    let service = b.service().with_budget(Duration::ZERO);
    let view = service.list(&b.key).await.unwrap();
    assert_eq!(view.unread, 5);
    assert!(view.mods.iter().all(|m| m.identity.is_none()));
    // Une lecture suivante, avec du temps, est complète.
    let full = b.list().await;
    assert_eq!(full.unread, 0);
    assert!(full.mods.iter().all(|m| m.identity.is_some()));
}

#[tokio::test]
async fn listing_never_changes_the_volume() {
    let b = bench(14, "fabric", Some("1.20.1")).await;
    b.put("mods/a.jar", &fabric("a", "1.0.0", "~1.20.1", ""));
    b.put("mods/junk.jar", b"x");
    let before = b.fs.paths(&b.key);
    let contents: Vec<_> = before
        .iter()
        .map(|path| b.fs.content(&b.key, path))
        .collect();
    let _ = b.list().await;
    let _ = b.list().await;
    assert_eq!(b.fs.paths(&b.key), before);
    let after: Vec<_> = before
        .iter()
        .map(|path| b.fs.content(&b.key, path))
        .collect();
    assert_eq!(after, contents);
}

#[tokio::test]
async fn a_server_installed_before_the_rows_existed_reads_its_stored_pack_once_to_know_its_mods() {
    let b = bench(15, "fabric", Some("1.20.1")).await;
    let from_pack = fabric("pack", "1.0.0", "~1.20.1", "");
    let by_hand = fabric("hand", "1.0.0", "~1.20.1", "");
    b.put("mods/pack.jar", &from_pack);
    b.put("mods/hand.jar", &by_hand);
    b.fs.seed_file(
        &b.key,
        "packs/pack.zip",
        &jar(&[("mods/pack.jar", &from_pack), ("config/a.toml", b"x")]),
    );
    let service = b.service();
    let view = service.list(&b.key).await.unwrap();
    assert_eq!(find(&view, "pack.jar").origin, ModOrigin::ServerPack);
    assert_eq!(find(&view, "hand.jar").origin, ModOrigin::Manual);
    assert_eq!(view.game.pack, PackKind::ServerPack);
    // Les lignes sont gardées : la base les rend, et le pack n'est pas relu.
    assert_eq!(b.origins.rows(&b.key).await.unwrap().len(), 1);
    let opened_before =
        b.fs.opened_paths()
            .iter()
            .filter(|p| p.as_str() == "packs/pack.zip")
            .count();
    let again = service.list(&b.key).await.unwrap();
    assert_eq!(find(&again, "pack.jar").origin, ModOrigin::ServerPack);
    let opened_after =
        b.fs.opened_paths()
            .iter()
            .filter(|p| p.as_str() == "packs/pack.zip")
            .count();
    assert_eq!((opened_before, opened_after), (1, 1));
}
