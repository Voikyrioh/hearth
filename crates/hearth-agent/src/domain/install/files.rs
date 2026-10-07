//! **Les noms de fichiers de Hearth, en un seul endroit.** Le dossier de données, le dossier du
//! binaire et celui de l'unité ne contiennent que ces noms (et leurs temporaires d'écriture) :
//! la désinstallation et le retour en arrière ne retirent que ce qui est listé ici, puis le
//! dossier s'il est vide. L'écriture (identité, base, binaire, unité) lit ses noms ici aussi : un
//! nom ne peut donc pas être écrit sans être purgé.

/// Certificat de l'agent.
pub const CERT_FILE: &str = "cert.pem";
/// Clé privée du certificat.
pub const KEY_FILE: &str = "key.pem";
/// Identifiant stable de l'installation.
pub const INSTALL_ID_FILE: &str = "install_id";
/// Verrou de création de l'identité.
pub const IDENTITY_LOCK_FILE: &str = "identity.lock";
/// La base SQLite.
pub const DATABASE_FILE: &str = "hearth.db";

/// Fichiers de l'identité (BR-INSTALL-004).
pub const IDENTITY_FILES: [&str; 4] = [CERT_FILE, KEY_FILE, INSTALL_ID_FILE, IDENTITY_LOCK_FILE];
/// Fichiers qui portent le contenu de l'identité (ceux dont un temporaire d'écriture existe).
pub const IDENTITY_CONTENT_FILES: [&str; 3] = [CERT_FILE, KEY_FILE, INSTALL_ID_FILE];

/// Fichiers de la base : `hearth.db` et ceux que SQLite y ajoute.
pub const DATABASE_FILES: [&str; 4] = [
    DATABASE_FILE,
    "hearth.db-wal",
    "hearth.db-shm",
    "hearth.db-journal",
];

/// **La liste exacte** de ce que Hearth écrit dans le dossier de données (le journal d'activité
/// vit dans la base).
pub const DATA_FILES: [&str; 8] = {
    // L'identité puis la base, sans les retaper : un nom ajouté à l'une est purgé.
    let mut all = [""; 8];
    let mut i = 0;
    while i < 4 {
        all[i] = IDENTITY_FILES[i];
        all[4 + i] = DATABASE_FILES[i];
        i += 1;
    }
    all
};

/// Dossier de la mise à jour dans le dossier de données (BR-UPDATE-015) : binaire déposé, copie du
/// superviseur, travail du superviseur, résultat. Un dossier dans le dossier de données plutôt que
/// `/tmp` : l'unité a `PrivateTmp=yes`, et le superviseur doit voir les mêmes fichiers.
pub const UPDATE_DIR: &str = "update";
/// Le nouveau binaire, déposé après vérification de la signature et de la somme.
pub const UPDATE_STAGED_FILE: &str = "hearth-agent.new";
/// La copie de l'ancien binaire qui joue le superviseur.
pub const UPDATE_SUPERVISOR_FILE: &str = "supervisor";
/// Ce que le superviseur doit faire (version attendue, chemins, qui a demandé).
pub const UPDATE_JOB_FILE: &str = "job.json";
/// Où en est le superviseur (étape en cours).
pub const UPDATE_STATE_FILE: &str = "state.json";
/// Le marqueur d'étape du superviseur (HRT-27) : jusqu'où il est allé, pour qu'une reprise continue
/// sans rien refaire. Écrit par un fichier voisin puis un renommage, `fsync` du fichier et du dossier.
pub const UPDATE_PHASE_FILE: &str = "phase.json";
/// Le dernier résultat, qui survit au redémarrage.
pub const UPDATE_LAST_FILE: &str = "last.json";
/// Verrou tenu par le superviseur tant qu'il travaille (relâché par le système s'il meurt).
pub const UPDATE_LOCK_FILE: &str = "lock";
/// Copie de la base prise service arrêté, avant l'échange des binaires (BR-UPDATE-029).
pub const UPDATE_DB_BACKUP_FILE: &str = "hearth.db.before";
/// Idem pour le journal de la base, s'il en reste un.
pub const UPDATE_WAL_BACKUP_FILE: &str = "hearth.db-wal.before";

/// Ce que la mise à jour écrit dans `UPDATE_DIR` (retiré par la désinstallation avec purge).
pub const UPDATE_FILES: [&str; 9] = [
    UPDATE_STAGED_FILE,
    UPDATE_SUPERVISOR_FILE,
    UPDATE_JOB_FILE,
    UPDATE_STATE_FILE,
    UPDATE_PHASE_FILE,
    UPDATE_LAST_FILE,
    UPDATE_LOCK_FILE,
    UPDATE_DB_BACKUP_FILE,
    UPDATE_WAL_BACKUP_FILE,
];

/// Temporaire d'écriture d'un fichier de la mise à jour (`last.json.<pid>.tmp`) : seulement un nom
/// que Hearth écrit, jamais un `*.tmp` quelconque.
pub fn is_update_temporary(name: &str) -> bool {
    name.strip_suffix(".tmp").is_some_and(|stem| {
        UPDATE_FILES.iter().any(|file| {
            stem.strip_prefix(file)
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
        })
    })
}

/// Début du nom d'un binaire en cours de copie : `.hearth-agent.new-<pid>`.
pub const BINARY_TEMP_PREFIX: &str = ".hearth-agent.new-";

/// Nom du fichier voisin d'un binaire pendant sa copie.
pub fn binary_temporary_name(pid: u32) -> String {
    format!("{BINARY_TEMP_PREFIX}{pid}")
}

/// Fichier voisin d'un binaire resté après une copie interrompue.
pub fn is_binary_temporary(name: &str) -> bool {
    name.strip_prefix(BINARY_TEMP_PREFIX)
        .is_some_and(|pid| !pid.is_empty() && pid.chars().all(|c| c.is_ascii_digit()))
}

/// Temporaire d'écriture de l'identité (`key.pem.<pid>.<n>.tmp`) : peut contenir une clé privée.
pub fn is_identity_temporary(name: &str) -> bool {
    name.ends_with(".tmp")
        && IDENTITY_CONTENT_FILES
            .iter()
            .any(|base| name.starts_with(&format!("{base}.")))
}

/// Temporaire d'écriture de la base remise par un retour arrière (`hearth.db.<pid>.tmp`).
pub fn is_database_temporary(name: &str) -> bool {
    name.strip_suffix(".tmp")
        .and_then(|stem| stem.rsplit_once('.'))
        .is_some_and(|(base, pid)| {
            DATABASE_FILES[..2].contains(&base)
                && !pid.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Extension du fichier voisin de l'unité pendant son écriture : `hearth-agent.service.new`.
pub const UNIT_TEMP_EXTENSION: &str = "service.new";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_list_is_the_identity_and_the_database() {
        let mut expected: Vec<&str> = IDENTITY_FILES.to_vec();
        expected.extend(DATABASE_FILES);
        assert_eq!(DATA_FILES.to_vec(), expected);
    }

    #[test]
    fn an_update_temporary_is_a_known_name_a_process_number_and_tmp_nothing_else() {
        assert!(is_update_temporary("last.json.4242.tmp"));
        assert!(is_update_temporary("hearth-agent.new.7.tmp"));
        for name in [
            "notes.tmp",
            "last.json.tmp",
            "last.json.x.tmp",
            "autre.1.tmp",
            "last.json.1",
        ] {
            assert!(!is_update_temporary(name), "{name}");
        }
    }

    #[test]
    fn identity_temporaries_are_recognised_and_nothing_else() {
        for name in [
            "key.pem.123.0.tmp",
            "cert.pem.9.4.tmp",
            "install_id.1.1.tmp",
        ] {
            assert!(is_identity_temporary(name), "{name}");
        }
        for name in ["key.pem", "key.pem.123.0", "notes.tmp", "hearth.db.1.2.tmp"] {
            assert!(!is_identity_temporary(name), "{name}");
        }
    }

    #[test]
    fn a_binary_temporary_is_the_prefix_and_a_process_number() {
        assert_eq!(binary_temporary_name(42), ".hearth-agent.new-42");
        assert!(is_binary_temporary(&binary_temporary_name(42)));
        for name in [
            ".hearth-agent.new-",
            ".hearth-agent.new-x",
            "hearth-agent",
            ".hearth-agent.previous",
        ] {
            assert!(!is_binary_temporary(name), "{name}");
        }
    }
}
