-- Les mods d'un serveur de jeu installé (HRT-256, ADR-0062, BR-MOD-020 à 037).
--
-- 1. Un genre d'opération de plus : `mods` (appliquer un lot de mods, HRT-258).
--    SQLite ne sait pas modifier un CHECK : `service_ops` est refaite à l'identique avec `mods` en plus.
--    `service_op_steps` la référence par `ON DELETE CASCADE` : retirer l'ancienne table viderait le journal
--    d'intention. Les étapes sont donc mises de côté puis remises (mêmes numéros, mêmes lignes). Les index sont
--    recréés tels qu'en 0012.
-- 2. `service_mod_origins` : d'où vient un fichier du dossier des mods (BR-MOD-033). Jamais un fichier dans le
--    volume du service (écrit par le jeu et ses mods : hostile) ; la base est écrite dans la transaction de fin
--    d'une opération, sous le verrou, et part avec le service.
-- 3. `backup_mod_sets` et `backup_mod_entries` : la liste des mods de chaque sauvegarde (BR-MOD-034, HRT-293),
--    vides jusqu'à HRT-293. Sans clé étrangère vers `backups` (même choix que le registre : le retrait est
--    explicite, dans la transaction de `BackupRepo`).
--
-- Les dates suivent la convention de 0001 (texte UTC à largeur fixe).

CREATE TABLE service_op_steps_keep AS SELECT * FROM service_op_steps;

CREATE TABLE service_ops_next (
    id             TEXT NOT NULL PRIMARY KEY,
    op_key         TEXT NOT NULL,
    service_key    TEXT NOT NULL,
    service_name   TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN
        ('start', 'stop', 'restart', 'deploy', 'update', 'delete', 'resources', 'backup', 'restore',
         'world_import', 'mods')),
    account        TEXT NOT NULL,
    plan_json      TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN
        ('accepted', 'running', 'awaiting_decision', 'cancelling', 'cancelled', 'rolling_back',
         'rolled_back', 'succeeded', 'failed', 'interrupted', 'needs_attention')),
    step           TEXT,
    decision_json  TEXT,
    decision_at    TEXT,
    progress_json  TEXT NOT NULL DEFAULT '{}',
    cause          TEXT NOT NULL DEFAULT '',
    forced         INTEGER NOT NULL DEFAULT 0 CHECK (forced IN (0, 1)),
    started_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    finished_at    TEXT
) STRICT;

INSERT INTO service_ops_next
SELECT id, op_key, service_key, service_name, kind, account, plan_json, status, step, decision_json,
       decision_at, progress_json, cause, forced, started_at, updated_at, finished_at
FROM service_ops;

DROP TABLE service_ops;
ALTER TABLE service_ops_next RENAME TO service_ops;

CREATE UNIQUE INDEX service_ops_open ON service_ops (service_key)
    WHERE status IN ('accepted', 'running', 'awaiting_decision', 'cancelling', 'rolling_back', 'needs_attention');
CREATE INDEX service_ops_op_key ON service_ops (op_key);
CREATE INDEX service_ops_finished_at ON service_ops (finished_at);

INSERT INTO service_op_steps (op_id, seq, step, state, undo_json, at)
SELECT op_id, seq, step, state, undo_json, at FROM service_op_steps_keep;
DROP TABLE service_op_steps_keep;

CREATE TABLE service_mod_origins (
    service_key  TEXT    NOT NULL CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    -- Le nom sans « .disabled », de l'alphabet sûr.
    file_name    TEXT    NOT NULL CHECK (length(file_name) BETWEEN 5 AND 128),
    size_bytes   INTEGER NOT NULL CHECK (size_bytes > 0),
    -- Pack serveur : CRC-32 de l'entrée du pack. Posé par Hearth : SHA-256 du fichier.
    crc32        INTEGER NULL,
    sha256       TEXT    NULL CHECK (sha256 IS NULL OR length(sha256) = 64),
    origin       TEXT    NOT NULL CHECK (origin IN ('server_pack', 'hearth')),
    recorded_at  TEXT    NOT NULL,
    PRIMARY KEY (service_key, file_name, origin),
    CHECK ((origin = 'hearth' AND sha256 IS NOT NULL) OR (origin = 'server_pack' AND crc32 IS NOT NULL))
) STRICT;

CREATE TABLE backup_mod_sets (
    backup_id    TEXT    NOT NULL PRIMARY KEY CHECK (length(backup_id) = 26),
    service_key  TEXT    NOT NULL CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    state        TEXT    NOT NULL CHECK (state IN ('recorded', 'partial', 'unreadable', 'not_in_backup')),
    loader       TEXT    NULL CHECK (loader IS NULL OR loader IN ('fabric', 'quilt', 'forge', 'neoforge')),
    minecraft    TEXT    NULL CHECK (minecraft IS NULL OR length(minecraft) BETWEEN 1 AND 32),
    active       INTEGER NOT NULL DEFAULT 0 CHECK (active >= 0),
    disabled     INTEGER NOT NULL DEFAULT 0 CHECK (disabled >= 0),
    retired      INTEGER NOT NULL DEFAULT 0 CHECK (retired >= 0),
    not_listed   INTEGER NOT NULL DEFAULT 0 CHECK (not_listed >= 0),
    recorded_at  TEXT    NOT NULL
) STRICT;

CREATE TABLE backup_mod_entries (
    backup_id     TEXT    NOT NULL CHECK (length(backup_id) = 26),
    rank          INTEGER NOT NULL CHECK (rank >= 0),
    -- NULL : nom hors de l'alphabet sûr (montré par son rang). 137 = 128 de nom plus « .disabled ».
    file_name     TEXT    NULL CHECK (file_name IS NULL OR length(file_name) BETWEEN 5 AND 137),
    place         TEXT    NOT NULL CHECK (place IN ('active', 'disabled', 'retired')),
    mod_id        TEXT    NULL CHECK (mod_id IS NULL OR length(mod_id) BETWEEN 1 AND 64),
    name          TEXT    NULL CHECK (name IS NULL OR length(name) BETWEEN 1 AND 64),
    version       TEXT    NULL CHECK (version IS NULL OR length(version) BETWEEN 1 AND 64),
    loader        TEXT    NULL CHECK (loader IS NULL OR loader IN ('fabric', 'quilt', 'forge', 'neoforge')),
    minecraft_req TEXT    NULL CHECK (minecraft_req IS NULL OR length(minecraft_req) <= 64),
    environment   TEXT    NULL CHECK (environment IS NULL OR environment IN ('both', 'server', 'client', 'unknown')),
    origin        TEXT    NOT NULL CHECK (origin IN ('server_pack', 'hearth', 'modrinth_pack', 'manual')),
    size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
    sha256        TEXT    NOT NULL CHECK (length(sha256) = 64),
    PRIMARY KEY (backup_id, rank)
) STRICT;
