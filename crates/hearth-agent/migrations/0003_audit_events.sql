-- Journal d'activité (HRT-05). Même convention de dates que 0001 (texte UTC à largeur fixe).
-- Une entrée n'est jamais modifiée (BR-AUDIT-009) : seul le nettoyage horaire en supprime
-- (BR-AUDIT-008). Les textes sont figés à l'écriture : l'identifiant du compte (le compte peut
-- disparaître ensuite), le libellé de l'action et la raison tels que l'administrateur les lit.
-- Aucun mot de passe, jeton ni clé n'entre ici : l'application n'écrit que des valeurs typées
-- (BR-AUDIT-005).

CREATE TABLE audit_events (
    -- Croissant et jamais réutilisé : sert de curseur de pagination.
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    at           TEXT NOT NULL,
    -- Identifiant du compte à l'origine de l'action ; absent pour la ligne de commande et pour
    -- une connexion refusée (l'identifiant saisi n'est jamais retenu : BR-AUDIT-005, 006).
    account      TEXT,
    origin_kind  TEXT NOT NULL CHECK (origin_kind IN ('client', 'cli', 'assistant')),
    -- Nom du poste (client seulement) et adresse IP vue par l'agent.
    origin_name  TEXT,
    origin_addr  TEXT,
    -- Code stable de l'action (`login`, `account.create`…) et son libellé.
    action       TEXT NOT NULL,
    action_label TEXT NOT NULL,
    target       TEXT,
    outcome      TEXT NOT NULL CHECK (outcome IN ('ok', 'denied', 'failed')),
    reason       TEXT,
    -- Entrée de synthèse : combien d'autres fois le même événement s'est produit en 1 minute.
    repeat_count INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX audit_events_at ON audit_events (at);
CREATE INDEX audit_events_account ON audit_events (account);
CREATE INDEX audit_events_action ON audit_events (action);
CREATE INDEX audit_events_outcome ON audit_events (outcome);

-- Aucune modification d'une entrée écrite.
CREATE TRIGGER audit_events_no_update BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'une entrée du journal ne se modifie pas');
END;

-- Recherche plein texte sur ce que l'administrateur voit (BR-AUDIT-016) : table miroir
-- (contenu externe) tenue à jour par les déclencheurs ci-dessous.
CREATE VIRTUAL TABLE audit_fts USING fts5 (
    account, origin_name, origin_addr, action_label, target, reason,
    content = 'audit_events', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

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
