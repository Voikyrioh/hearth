#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Un serveur de jeu déjà installé depuis la fiche de Terraria, sans passer par l'installation (HRT-108) : sa
//! description durable (fiche, ports, montage du dossier de réglages) et son fichier de réglages dans le faux
//! `SafeFs`. Sert aux tests des routes de réglages et à la matrice des actes.

use std::sync::Arc;

use hearth_agent::application::catalog::CatalogService;
use hearth_agent::application::ports::{Ownership, Perms, SafeFs};
use hearth_agent::application::secret_vault::SecretVault;
use hearth_agent::application::service_specs::ServiceSpecs;
use hearth_agent::domain::files::RelPath;
use hearth_agent::domain::services::ServiceKey;
use hearth_agent::domain::services::spec::{
    SpecBody, SpecFiche, SpecImage, SpecMount, SpecMountKind, SpecPort, SpecProtocol, SpecRestart,
    SpecSecrets, SpecSettings,
};
use hearth_agent::infrastructure::sealer::RingSealerFactory;
use hearth_agent::infrastructure::services_key::FileServicesKeyStore;
use hearth_agent::infrastructure::sqlite::SqliteSpecRepo;

use super::Env;
use super::fake_safe_fs::FakeSafeFs;

/// Le contenu du fichier de réglages d'un serveur neuf.
pub const INITIAL_FILE: &[u8] = b"# vide\n";

/// La clé d'un service de jeu de test.
pub fn key_of(n: u32) -> ServiceKey {
    ServiceKey::parse(&format!("h_01JZ00000000000000000001{n:02}")).expect("clé")
}

/// Les descriptions durables, au-dessus de la base du banc.
pub fn specs_of(env: &Env) -> Arc<ServiceSpecs> {
    let spec_repo = Arc::new(SqliteSpecRepo::new(env.db.pool().clone()));
    Arc::new(ServiceSpecs::new(
        spec_repo.clone(),
        Arc::new(SecretVault::new(
            Arc::new(FileServicesKeyStore::new(env.dir.path())),
            spec_repo,
            Arc::new(RingSealerFactory),
        )),
        env.clock.clone(),
    ))
}

/// Installe (sans moteur) le service `n` : description, dossier `config` et fichier `serverconfig.txt`.
pub async fn terraria_service(env: &Env, fs: &FakeSafeFs, n: u32) -> ServiceKey {
    terraria_service_with(env, fs, n, INITIAL_FILE).await
}

pub async fn terraria_service_with(env: &Env, fs: &FakeSafeFs, n: u32, file: &[u8]) -> ServiceKey {
    let key = key_of(n);
    let catalog = CatalogService::embedded(true);
    let digest = catalog.fiche("terraria").expect("terraria").digest_hex();
    let body = SpecBody {
        name: format!("terraria-{n}"),
        image: SpecImage {
            repository: "ryshe/terraria".into(),
            tag: "vanilla-1.4.5.8".into(),
            digest: None,
        },
        ports: vec![SpecPort {
            host_port: 7777,
            container_port: 7777,
            protocol: SpecProtocol::Tcp,
        }],
        mounts: vec![SpecMount {
            kind: SpecMountKind::Managed,
            source: "config".into(),
            target: "/config".into(),
            read_only: true,
        }],
        restart: SpecRestart::UnlessStopped,
        stop_grace_s: Some(10),
        stdin_open: true,
        cpu_millis: Some(1000),
        memory_bytes: Some(1 << 30),
        settings: SpecSettings::default(),
        fiche: Some(SpecFiche::new("terraria", 1, digest)),
    };
    specs_of(env)
        .save(&key, None, body, &SpecSecrets::default(), "marie")
        .await
        .expect("description");
    put_file(fs, &key, file);
    key
}

/// Pose (ou remplace) le fichier de réglages d'un service, comme le ferait le jeu ou une main.
pub fn put_file(fs: &FakeSafeFs, key: &ServiceKey, bytes: &[u8]) {
    let root = fs.ensure_service_root(key).expect("racine");
    let owner = Ownership::new(0, 0).expect("propriétaire");
    let dir = Perms::new(owner, 0o777).expect("droits");
    root.mkdir(&RelPath::parse("config").unwrap(), dir)
        .expect("dossier");
    let file = Perms::new(owner, 0o600).expect("droits");
    root.write_atomic(
        &RelPath::parse("config/serverconfig.txt").unwrap(),
        bytes,
        file,
    )
    .expect("fichier");
}

/// Le fichier de réglages tel qu'il est.
pub fn file_of(fs: &FakeSafeFs, key: &ServiceKey) -> Vec<u8> {
    fs.content(key, "config/serverconfig.txt")
        .expect("fichier de réglages")
}
