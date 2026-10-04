-- Sessions et garde de rôle (HRT-04) : compteurs de tentatives de connexion, opérations suivies
-- par clé, jetons de sessions révoquées. Même convention de dates que 0001 (texte UTC à largeur
-- fixe). La table audit_events arrive avec HRT-05.

-- Verrouillage progressif : un compteur par couple (identifiant, adresse séparés par un caractère de contrôle) (BR-CONN-006/007).
-- La clé existe aussi pour les identifiants inconnus : le verrouillage ne révèle rien. Une
-- ligne « addr:<adresse> » compte les échecs d'une adresse, tous identifiants confondus.
CREATE TABLE login_attempts (
    key          TEXT NOT NULL PRIMARY KEY,
    failures     INTEGER NOT NULL,
    locked_until TEXT,
    updated_at   TEXT NOT NULL,
    -- Ouverture de la fenêtre de comptage (compteur par adresse « addr:… » seulement).
    window_started_at TEXT
) STRICT;

-- Opérations suivies par la clé du client (BR-RESIL-010). La clé est celle d'un compte :
-- clé primaire (account_id, id). `account_id` n'est pas une clé étrangère : l'opération
-- « supprimer mon compte » doit survivre au compte. `request_hash` lie la clé à une requête
-- (méthode, chemin, corps). `result_json` : statut HTTP et corps de la réponse.
CREATE TABLE operations (
    id           TEXT NOT NULL,
    account_id   TEXT NOT NULL,
    kind         TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'interrupted')),
    result_json  TEXT,
    created_at   TEXT NOT NULL,
    finished_at  TEXT,
    PRIMARY KEY (account_id, id)
) STRICT;

CREATE INDEX operations_created_at ON operations (created_at);

-- Empreintes des jetons des sessions fermées par l'administration : elles distinguent
-- SESSION_REVOKED de SESSION_EXPIRED (BR-RESIL-014). Purgées après 90 jours.
CREATE TABLE revoked_sessions (
    token_hash TEXT NOT NULL PRIMARY KEY,
    revoked_at TEXT NOT NULL
) STRICT;

CREATE INDEX sessions_expires_at ON sessions (expires_at);
