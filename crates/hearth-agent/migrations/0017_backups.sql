-- Les sauvegardes d'un service (HRT-97, ADR-0047, BR-BACKUP-004, 012, 013).
--
-- `backups` est le REGISTRE : le disque dit si une archive existe, cette table dit si elle est à Hearth. Hearth ne
-- lit comme sauvegarde, ne renomme et ne supprime qu'un fichier inscrit ici avec son numéro d'inode ET dont le
-- manifeste voisin a l'empreinte de la ligne (`manifest_sha256`), quel que soit le nom du fichier. Un fichier de la
-- racine des sauvegardes qui n'est dans aucune ligne n'existe pas pour Hearth.
--
-- Une ligne `writing` est une archive en cours d'écriture (`<id>.tar.gz.part`) ; au démarrage, les lignes `writing`
-- sont retirées (le fichier partiel l'est par le journal des opérations, après vérification de l'inode). Une ligne
-- `ready` dont l'archive a disparu est « disparue » (listée, retirée du registre par le compte).
--
-- La note du compte est SCELLÉE (ChaCha20-Poly1305, clé `services.key`, données associées = service, révision 0,
-- champ `backup-note:<id>`) : elle n'est ni en clair ici, ni au manifeste, ni au journal d'activité (BR-BACKUP-012).
-- Les dates suivent la convention de 0001 (texte UTC à largeur fixe). Le périphérique et l'inode sont des entiers 64
-- bits non signés rangés dans un entier signé (même suite de bits).

CREATE TABLE backups (
    id              TEXT NOT NULL PRIMARY KEY CHECK (length(id) = 26),
    -- La clé du service (`h_<ULID>`) : l'identité du service, qui survit à « Recréer » et « Modifier » (BR-BACKUP-016).
    -- Pas de clé étrangère : les sauvegardes survivent à la description d'un service supprimé (spec de suppression d'un service : « sans lien »).
    service_key     TEXT NOT NULL CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    -- Le nom du service au moment de la sauvegarde, figé.
    service_name    TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('writing', 'ready')),
    created_at      TEXT NOT NULL,
    created_by      TEXT NOT NULL,
    dev             INTEGER NOT NULL,
    ino             INTEGER NOT NULL,
    archive_bytes   INTEGER NOT NULL DEFAULT 0 CHECK (archive_bytes >= 0),
    data_bytes      INTEGER NOT NULL DEFAULT 0 CHECK (data_bytes >= 0),
    files           INTEGER NOT NULL DEFAULT 0 CHECK (files >= 0),
    mode            TEXT NOT NULL CHECK (mode IN ('stopped', 'hot', 'stop_copy_start')),
    -- Liste JSON de repères (`["hot","maybe_inconsistent"]`), jamais un texte libre.
    flags_json      TEXT NOT NULL DEFAULT '[]',
    changed_files   INTEGER NOT NULL DEFAULT 0 CHECK (changed_files >= 0),
    image           TEXT NOT NULL DEFAULT '',
    integrity       TEXT NOT NULL DEFAULT 'unchecked' CHECK (integrity IN ('unchecked', 'verified', 'corrupt')),
    verified_at     TEXT,
    manifest_sha256 TEXT NOT NULL DEFAULT '',
    archive_sha256  TEXT NOT NULL DEFAULT '',
    note_nonce      BLOB CHECK (note_nonce IS NULL OR length(note_nonce) = 12),
    note_sealed     BLOB CHECK (note_sealed IS NULL OR length(note_sealed) >= 16)
) STRICT;

CREATE INDEX backups_by_service ON backups (service_key, created_at);

-- Ceinture du « nonce jamais réutilisé » pour les notes scellées.
CREATE UNIQUE INDEX backups_note_nonce ON backups (note_nonce) WHERE note_nonce IS NOT NULL;

-- Les éléments ignorés d'un service (BR-BACKUP-003) : ceux qu'un compte a ajoutés, et les ignorés par défaut
-- qu'il a fait entrer dans les sauvegardes. Des chemins DANS LE SERVICE (cible de montage et chemin relatif), pas
-- ceux de la machine ; jamais écrits au journal d'activité (leur nombre seulement).
CREATE TABLE backup_settings (
    service_key   TEXT NOT NULL PRIMARY KEY CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    ignored_json  TEXT NOT NULL DEFAULT '[]',
    included_json TEXT NOT NULL DEFAULT '[]',
    updated_by    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
) STRICT;
