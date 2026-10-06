-- Identité d'appareil et mode attaque (HRT-22, ADR-0023 ; la story entière, une seule migration).
-- Même convention de dates que 0001 (texte UTC à largeur fixe). Migration ADDITIVE sur la 0004 :
-- trois tables neuves et quatre colonnes nulles par défaut ; rien n'est renommé, supprimé ni
-- réécrit, et la 0004 n'est pas touchée (une migration publiée ne se modifie jamais). Un ancien
-- binaire n'a rien à y changer ; s'il doit revenir, c'est la copie de la base d'avant l'échange
-- que le superviseur remet (BR-UPDATE-029).

-- Postes de confiance d'un compte (BR-TRUST-004, 022) : un par clé d'appareil inscrite. Seule la
-- clé PUBLIQUE est gardée (32 octets) ; `key_id` est son empreinte (16 octets de SHA-256, 32
-- caractères hexadécimaux), unique par compte. 8 postes au plus par compte (borne tenue par le
-- domaine). Supprimés avec le compte.
CREATE TABLE trusted_devices (
    id             TEXT NOT NULL PRIMARY KEY,
    account_id     TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    key_id         TEXT NOT NULL,
    algorithm      TEXT NOT NULL,
    public_key     BLOB NOT NULL,
    name           TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    last_proved_at TEXT NOT NULL,
    last_addr      TEXT NOT NULL
) STRICT;

CREATE UNIQUE INDEX trusted_devices_account_key ON trusted_devices (account_id, key_id);
-- Recherche d'une clé tous comptes confondus (une clé n'est jamais confiée à deux comptes).
CREATE INDEX trusted_devices_key ON trusted_devices (key_id);
-- Purge : 90 jours sans preuve.
CREATE INDEX trusted_devices_last_proved_at ON trusted_devices (last_proved_at);

-- Mode attaque : une ligne, toujours la même (HRT-25 l'écrira ; ici elle existe, inactive, pour
-- que l'inscription sache qu'elle est gelée quand il sera actif).
CREATE TABLE attack_mode (
    id                    INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    active                INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
    activation_id         TEXT,
    activated_at          TEXT,
    activated_by          TEXT,
    ended_at              TEXT,
    ended_how             TEXT CHECK (ended_how IS NULL OR ended_how IN ('manual', 'auto', 'cli')),
    last_boot_id          TEXT,
    window_boot_id        TEXT,
    remote_reboot_boot_id TEXT
) STRICT;

INSERT INTO attack_mode (id, active) VALUES (1, 0);

-- Essais uniques du mode attaque (HRT-25) : un par critère présenté, par compte et par
-- activation. 16 lignes au plus par compte et par activation (8 adresses, 8 postes).
CREATE TABLE attack_trials (
    activation_id TEXT NOT NULL,
    account_id    TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('address', 'key')),
    subject       TEXT NOT NULL,
    used_at       TEXT NOT NULL,
    outcome       TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
    PRIMARY KEY (activation_id, account_id, kind, subject)
) STRICT;

-- Adresses retenues (la table de la 0004) : le poste à clé auquel l'adresse est liée (nul pour une
-- adresse apprise sans clé), et le dernier usage reconnu d'une session depuis cette adresse. Une
-- adresse reste retenue 30 jours après le plus récent de `last_success_at` et `last_used_at`.
-- La ligne suit son poste : retirer le poste l'efface.
ALTER TABLE known_addresses ADD COLUMN device_id TEXT REFERENCES trusted_devices (id) ON DELETE CASCADE;
ALTER TABLE known_addresses ADD COLUMN last_used_at TEXT;
CREATE INDEX known_addresses_device ON known_addresses (device_id);

-- Ralentissement par identifiant (la table de la 0004) : l'instant où l'alerte de l'épisode a été
-- signalée (HRT-24).
ALTER TABLE identifier_slowdowns ADD COLUMN alerted_at TEXT;

-- Le poste dont la clé a ouvert ou prouvé la session. Retirer un poste ferme ses sessions
-- (explicitement) ; l'oublier par la purge de 90 jours laisse la session ouverte, détachée.
ALTER TABLE sessions ADD COLUMN device_id TEXT REFERENCES trusted_devices (id) ON DELETE SET NULL;
CREATE INDEX sessions_device ON sessions (device_id);
