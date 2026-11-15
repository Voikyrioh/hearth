-- Les transferts reprenables (HRT-95, ADR-0048, BR-FILES-019, BR-MOD-015).
--
-- `transfers` est le REGISTRE de la zone d'attente de l'agent : le disque dit si un fichier existe, cette table dit
-- s'il est à Hearth. Hearth ne retire un fichier de la zone d'attente que s'il est inscrit ici avec son numéro
-- d'inode (la partie) ou son inode, sa taille ET sa date de modification (le fichier complet).
--
-- Un dépôt est `receiving` tant que des morceaux manquent, puis `complete` : le fichier attend son consommateur
-- (lecture d'archive, pack de mods). Dans les deux cas, `updated_at` est la dernière activité et le fichier est retiré
-- 24 heures plus tard s'il n'a pas été repris ou consommé.
--
-- Aucun NOM de fichier du client n'est gardé : le nom annoncé sert à juger et à signer la demande, jamais à ranger
-- (ce qui décide du genre d'un fichier est sa lecture). Les empreintes de morceaux ne sont PAS ici : elles vivent à
-- côté de la partie (`<id>.dig`, 32 octets par morceau) ; `received_chunks` n'est que la dernière valeur connue de
-- la base, le disque fait foi à la reprise.
--
-- Les dates suivent la convention de 0001 (texte UTC à largeur fixe). Le périphérique et l'inode sont des entiers 64
-- bits non signés rangés dans un entier signé (même suite de bits).

CREATE TABLE transfers (
    id                TEXT NOT NULL PRIMARY KEY CHECK (length(id) = 26),
    -- L'identifiant technique du compte qui a ouvert le dépôt. Pas de clé étrangère : un dépôt survit un temps à la
    -- suppression de son compte et part avec le délai de 24 heures.
    account_id        TEXT NOT NULL,
    state             TEXT NOT NULL CHECK (state IN ('receiving', 'complete')),
    size              INTEGER NOT NULL CHECK (size > 0),
    received_chunks   INTEGER NOT NULL DEFAULT 0 CHECK (received_chunks >= 0),
    dev               INTEGER NOT NULL,
    ino               INTEGER NOT NULL,
    -- Pour un dépôt complet : l'empreinte de l'ensemble (SHA-256 de la suite des empreintes de morceaux), en
    -- hexadécimal, et la date de modification du fichier publié (nanosecondes Unix).
    tree_digest       TEXT NOT NULL DEFAULT '',
    complete_mtime_ns INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
) STRICT;

CREATE INDEX transfers_by_account ON transfers (account_id);
CREATE INDEX transfers_by_activity ON transfers (updated_at);
