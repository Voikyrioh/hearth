-- Les réglages voulus d'un serveur de jeu créé depuis une fiche (HRT-108, ADR-0051, BR-FICHE-037, 048).
--
-- Deux vérités sans recouvrement : le FICHIER du jeu fait foi pour ses octets, cette table pour les valeurs que le
-- compte VEUT. Un écart entre les deux est montré, jamais corrigé seul. Une migration additive : deux tables
-- neuves, rien d'existant ne bouge. Un service sans ligne ici reprend ses réglages de sa description durable
-- (`service_specs.spec_json`, `fiche_extra.answers`), écrits à l'installation.
--
-- Aucune colonne ne reçoit la valeur d'un secret : `answers_json` ne garde que des valeurs non secrètes et le
-- MARQUEUR « secret défini » ; la valeur d'un secret change dans `game_setting_secrets`, scellée (ChaCha20-Poly1305,
-- clé `services.key`, données associées = service, révision 0, champ `answer:<réglage>`). Un réglage inconnu de la
-- fiche n'est copié nulle part : sa valeur reste dans le fichier du jeu.

CREATE TABLE game_settings (
    service_key      TEXT NOT NULL PRIMARY KEY CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    -- Numéro de la dernière écriture ; l'enregistrement suivant doit porter ce numéro (SETTINGS_CONFLICT sinon).
    settings_rev     INTEGER NOT NULL CHECK (settings_rev >= 1),
    -- `[{"id":"motd","v":"Bonjour"},{"id":"password","s":true}]` ; un réglage absent est « non écrit ».
    answers_json     TEXT NOT NULL,
    -- SHA-256 hexadécimal du fichier tel que Hearth l'a écrit en dernier (BR-FICHE-048), nul avant la première écriture.
    file_fingerprint TEXT,
    updated_by       TEXT NOT NULL,
    -- Même convention de dates que 0001 (texte UTC à largeur fixe).
    updated_at       TEXT NOT NULL
) STRICT;

CREATE TABLE game_setting_secrets (
    service_key TEXT NOT NULL REFERENCES game_settings (service_key) ON DELETE CASCADE,
    setting_id  TEXT NOT NULL,
    format      INTEGER NOT NULL DEFAULT 1 CHECK (format >= 1),
    nonce       BLOB NOT NULL CHECK (length(nonce) = 12),
    sealed      BLOB NOT NULL CHECK (length(sealed) >= 16),
    PRIMARY KEY (service_key, setting_id)
) STRICT;

-- Ceinture du « nonce jamais réutilisé ».
CREATE UNIQUE INDEX game_setting_secrets_nonce ON game_setting_secrets (nonce);

-- Les réglages d'un service partent avec sa description : quand la dernière révision de `service_specs` d'une clé est
-- effacée (suppression du service), la ligne `game_settings` et, par la clé étrangère, ses scellés partent aussi.
-- Une clé étrangère directe est impossible : la clé primaire de `service_specs` est (service_key, rev).
CREATE TRIGGER game_settings_forget_with_specs AFTER DELETE ON service_specs
WHEN NOT EXISTS (SELECT 1 FROM service_specs WHERE service_key = OLD.service_key)
BEGIN
    DELETE FROM game_settings WHERE service_key = OLD.service_key;
END;
