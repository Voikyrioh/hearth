//! `cargo xtask e2e-update` : la mise à jour de l'agent à distance sur une vraie machine jetable
//! (Debian avec systemd réellement démarré), dans la lignée de `e2e-install`.
//!
//! Prépare, dans `target/e2e-update/dist/` :
//! - une paire de clés minisign **jetable** (la clé secrète ne quitte jamais la mémoire de cette
//!   tâche), et l'agent construit deux fois avec la clé publique jetable (`HEARTH_UPDATE_PUBKEY_FILE`),
//!   en versions 0.1.0 (celui qui est installé) et 0.2.0 (la mise à jour) (`HEARTH_AGENT_VERSION`) ;
//! - leurs signatures, une signature d'une AUTRE clé du même fichier, et un faux agent « muet »
//!   (signé, il annonce 0.3.0 mais ne répond jamais) pour le retour automatique ;
//!
//! Puis exécute `deploy/e2e/scenario-update.sh` sur la machine. Une construction de publication ne
//! reçoit jamais ces variables : la clé embarquée est celle du dépôt (`update-key.pub`).

use std::fs;
use std::io::Cursor;

use crate::agent::{self, Build};
use crate::docker;

/// Le faux agent : `--version` annonce 0.3.0, tout le reste attend sans jamais écouter.
const MUTE: &str = "#!/bin/sh\nif [ \"${1:-}\" = \"--version\" ]; then\n  echo \"hearth-agent 0.3.0\"\n  exit 0\nfi\nexec sleep 3600\n";

pub fn run() -> Result<(), String> {
    let root = docker::repo_root()?;
    let work = root.join("target").join("e2e-update");
    let dist = work.join("dist");
    fs::create_dir_all(&dist)
        .map_err(|error| format!("création de {} impossible : {error}", dist.display()))?;

    // Deux exécutions en même temps se réécriraient la clé de test pendant que l'autre construit
    // ses agents (la demande « somme fausse » prendrait 422 au lieu de 202) : un verrou de fichier,
    // relâché par le système à la fin du processus, même tué.
    let lock = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(work.join("lock"))
        .map_err(|error| format!("verrou de e2e-update illisible : {error}"))?;
    lock.try_lock().map_err(|_| {
        "une autre exécution de e2e-update est en cours (target/e2e-update/lock)".to_owned()
    })?;

    println!("clés minisign jetables...");
    let ours = minisign::KeyPair::generate_unencrypted_keypair()
        .map_err(|error| format!("génération de clé impossible : {error}"))?;
    let theirs = minisign::KeyPair::generate_unencrypted_keypair()
        .map_err(|error| format!("génération de clé impossible : {error}"))?;
    let public = ours
        .pk
        .to_box()
        .map_err(|error| format!("clé publique illisible : {error}"))?
        .to_string();
    fs::write(work.join("test.pub"), public).map_err(|error| format!("écriture : {error}"))?;

    // La clé publique est lue dans le conteneur : le dépôt y est monté sur /src.
    let key_in_container = "/src/target/e2e-update/test.pub".to_owned();
    for (out_name, version) in [
        ("hearth-agent-old", "0.1.0"),
        ("hearth-agent-new", "0.2.0"),
        ("hearth-agent-next", "0.2.1"),
    ] {
        agent::build(&Build {
            dist: dist.clone(),
            out_name: out_name.to_owned(),
            expect_version: version.to_owned(),
            expect_key: None,
            env: vec![
                // Le serveur de versions du scénario est sur le bouclage : seule cette construction
                // de test ouvre les adresses locales (BR-UPDATE-027).
                (
                    "HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES".to_owned(),
                    "1".to_owned(),
                ),
                (
                    "HEARTH_UPDATE_PUBKEY_FILE".to_owned(),
                    key_in_container.clone(),
                ),
                ("HEARTH_AGENT_VERSION".to_owned(), version.to_owned()),
            ],
        })?;
    }

    let new =
        fs::read(dist.join("hearth-agent-new")).map_err(|error| format!("lecture : {error}"))?;
    fs::write(dist.join("mute"), MUTE).map_err(|error| format!("écriture : {error}"))?;
    sign(&ours.sk, &new, &dist.join("new.minisig"))?;
    sign(&theirs.sk, &new, &dist.join("new.other.minisig"))?;
    sign(&ours.sk, MUTE.as_bytes(), &dist.join("mute.minisig"))?;
    let next =
        fs::read(dist.join("hearth-agent-next")).map_err(|error| format!("lecture : {error}"))?;
    sign(&ours.sk, &next, &dist.join("next.minisig"))?;

    crate::e2e::machine(
        &dist,
        "/deploy/e2e/scenario-update.sh",
        "journalctl -u hearth-agent -u hearth-agent-update --no-pager 2>/dev/null | tail -n 60 || true",
    )?;
    println!("mise à jour de l'agent à distance : vert");
    Ok(())
}

fn sign(secret: &minisign::SecretKey, data: &[u8], path: &std::path::Path) -> Result<(), String> {
    let signature = minisign::sign(None, secret, Cursor::new(data), None, None)
        .map_err(|error| format!("signature impossible : {error}"))?;
    fs::write(path, signature.to_string())
        .map_err(|error| format!("écriture de {} : {error}", path.display()))
}
