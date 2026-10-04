-- Comptes, sessions et métadonnées (HRT-03). Les tables login_attempts, operations et
-- audit_events arrivent avec les tickets qui en ont besoin.
-- Dates : texte à largeur fixe en UTC, `YYYY-MM-DDTHH:MM:SS.mmmZ` (fractions toujours
-- présentes) : l'ordre du texte est l'ordre chronologique. Identifiants techniques : ULID.

CREATE TABLE accounts (
    id                  TEXT NOT NULL PRIMARY KEY,
    -- Minuscules (normalisées par le domaine) ; la collation rend l'unicité insensible à la casse
    -- même si une insertion contournait le domaine.
    username            TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash       TEXT NOT NULL,
    role                TEXT NOT NULL CHECK (role IN ('admin', 'readonly')),
    created_at          TEXT NOT NULL,
    password_changed_at TEXT NOT NULL,
    -- Renseignée à chaque connexion réussie (HRT-04).
    last_login_at       TEXT
) STRICT;

CREATE TABLE sessions (
    id           TEXT NOT NULL PRIMARY KEY,
    account_id   TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,
    client_name  TEXT NOT NULL,
    client_addr  TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at   TEXT NOT NULL
) STRICT;

CREATE INDEX sessions_account_id ON sessions (account_id);

CREATE TABLE meta (
    key   TEXT NOT NULL PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;
