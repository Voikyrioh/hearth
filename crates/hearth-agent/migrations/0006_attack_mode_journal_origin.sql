-- Mode attaque (HRT-25, ADR-0025) : ce que la 0005 n'avait pas prévu. Même convention de dates que
-- 0001 (texte UTC à largeur fixe). Une seule migration pour ce ticket ; aucune donnée n'est perdue ni
-- réécrite, et la 0005 n'est pas touchée (une migration publiée ne se modifie jamais). Un ancien
-- binaire n'a rien à y changer ; s'il doit revenir, c'est la copie de la base d'avant l'échange que
-- le superviseur remet (BR-UPDATE-029).
--
-- 1. Journal d'activité : une origine « système » (fin d'alerte levée par l'agent, sortie automatique
--    du mode attaque, suspension et reprise) et M, le nombre d'adresses d'une synthèse « N tentatives
--    depuis M adresses », dans un champ typé au lieu du texte de la raison.
--
--    SQLite ne sait pas modifier une contrainte CHECK : la table est refaite à l'identique (mêmes
--    colonnes, mêmes identifiants, même suite d'identifiants, jamais réutilisés), avec l'origine
--    `system` en plus et la colonne `repeat_addresses` (0 pour toute entrée déjà écrite). Les
--    déclencheurs et l'index sont recréés tels qu'en 0003 ; la recherche plein texte est reconstruite
--    depuis la table refaite.

DROP TRIGGER audit_events_fts_insert;
DROP TRIGGER audit_events_fts_delete;
DROP TRIGGER audit_events_no_update;
DROP INDEX audit_events_at;

CREATE TABLE audit_events_next (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    at               TEXT NOT NULL,
    account          TEXT,
    origin_kind      TEXT NOT NULL CHECK (origin_kind IN ('client', 'cli', 'assistant', 'system')),
    origin_name      TEXT,
    origin_addr      TEXT,
    action           TEXT NOT NULL,
    action_label     TEXT NOT NULL,
    target           TEXT,
    outcome          TEXT NOT NULL CHECK (outcome IN ('ok', 'denied', 'failed')),
    reason           TEXT,
    repeat_count     INTEGER NOT NULL DEFAULT 0,
    -- Synthèse « N tentatives depuis M adresses » : M ; 0 pour toute autre entrée.
    repeat_addresses INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO audit_events_next
    (id, at, account, origin_kind, origin_name, origin_addr, action, action_label, target,
     outcome, reason, repeat_count)
SELECT id, at, account, origin_kind, origin_name, origin_addr, action, action_label, target,
       outcome, reason, repeat_count
FROM audit_events;

-- Les identifiants ne sont jamais réutilisés : la suite continue là où l'ancienne table en était,
-- même si ses dernières entrées ont été purgées.
UPDATE sqlite_sequence
SET seq = MAX(seq, COALESCE((SELECT seq FROM sqlite_sequence WHERE name = 'audit_events'), 0))
WHERE name = 'audit_events_next';

DROP TABLE audit_events;
ALTER TABLE audit_events_next RENAME TO audit_events;

CREATE INDEX audit_events_at ON audit_events (at);

CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'une entrée du journal ne se modifie pas');
END;

CREATE TRIGGER audit_events_fts_insert AFTER INSERT ON audit_events
BEGIN
    INSERT INTO audit_fts (rowid, account, origin_name, origin_addr, action_label, target, reason)
    VALUES (new.id, new.account, new.origin_name, new.origin_addr, new.action_label, new.target, new.reason);
END;

CREATE TRIGGER audit_events_fts_delete AFTER DELETE ON audit_events
BEGIN
    INSERT INTO audit_fts (audit_fts, rowid, account, origin_name, origin_addr, action_label, target, reason)
    VALUES ('delete', old.id, old.account, old.origin_name, old.origin_addr, old.action_label, old.target, old.reason);
END;

INSERT INTO audit_fts (audit_fts) VALUES ('rebuild');

-- 2. Mode attaque : la fin de la dernière activation, mesurée sur le temps écoulé depuis le démarrage
--    du noyau (identifiant de démarrage et secondes d'`/proc/uptime`), pour que « une réactivation moins
--    de 30 minutes après la fin ne rend pas les essais » ne dépende d'aucune horloge murale tant que la
--    machine n'a pas redémarré. Nulles tant que le mode n'a jamais été désactivé.
ALTER TABLE attack_mode ADD COLUMN ended_boot_id TEXT;
ALTER TABLE attack_mode ADD COLUMN ended_uptime_s INTEGER;
