-- Description durable des services créés par Hearth et journal des opérations (HRT-83, ADR-0044,
-- ADR-0045). Même convention de dates que 0001 (texte UTC à largeur fixe).
--
-- NUMÉRO : `0011` est pris par la branche des services adoptés, pas encore fusionnée. Cette migration
-- ne dépend d'AUCUNE table de `0011` (ni ne la crée). SQLx applique toute migration absente de
-- `_sqlx_migrations`, quel que soit l'ordre des numéros : `0011` pourra arriver après `0012` sans rien
-- casser, et inversement.
--
-- Elle REMPLACE `service_actions` et `deployments` (créées par 0009, jamais écrites par aucun code :
-- aucune requête de l'agent ne les nomme) par un seul modèle d'opération. Elle ne détruit jamais de
-- données : si l'une des deux tables contenait une ligne, la garde ci-dessous fait échouer la migration
-- tout entière (SQLx l'exécute dans une transaction : la base reste telle qu'elle était, à `0010`).
-- Les tables `service_intents` et `supervised_services` sont gardées telles quelles.
--
-- Aucune colonne ne reçoit une valeur de variable, un nom de variable, une commande ni une réponse
-- secrète en clair : ils ne vivent dans `service_spec_secrets` que SCELLÉS (ChaCha20-Poly1305, clé dans
-- `services.key`, hors de la base). Le nom d'un champ scellé (`env:3`, `command`, `answer:<id>`) ne dit
-- rien du contenu.

-- Garde : refuse de supprimer une table qui contiendrait des lignes.
CREATE TABLE migration_0012_guard (ok INTEGER NOT NULL CHECK (ok = 1)) STRICT;
INSERT INTO migration_0012_guard (ok)
SELECT CASE
    WHEN EXISTS (SELECT 1 FROM service_actions) OR EXISTS (SELECT 1 FROM deployments) THEN 0
    ELSE 1
END;
DROP TABLE migration_0012_guard;

DROP TABLE service_actions;
DROP TABLE deployments;

-- Une ligne par RÉVISION d'une description. La révision courante porte `current = 1` ; les
-- précédentes restent lisibles (BR-EDIT-035) jusqu'à ce que l'ancien conteneur soit supprimé.
CREATE TABLE service_specs (
    service_key    TEXT NOT NULL,
    rev            INTEGER NOT NULL CHECK (rev >= 1),
    name           TEXT NOT NULL,
    -- Nom en minuscules : deux services du même nom ne coexistent pas, même si le verrou en mémoire
    -- avait un défaut (BR-DEL-019).
    name_folded    TEXT NOT NULL,
    -- La description SANS secret : image, ports, montages, limites, politique de redémarrage.
    spec_json      TEXT NOT NULL,
    -- Seuil du disque, éléments ignorés des sauvegardes, adresse publique.
    settings_json  TEXT NOT NULL DEFAULT '{}',
    -- Nuls hors service de fiche.
    fiche_id       TEXT,
    fiche_version  INTEGER,
    fiche_digest   TEXT,
    -- Identifiant du compte, figé : le compte peut disparaître ensuite.
    updated_by     TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    current        INTEGER NOT NULL CHECK (current IN (0, 1)),
    PRIMARY KEY (service_key, rev)
) STRICT;

-- Une seule révision courante par service, un seul service courant par nom.
CREATE UNIQUE INDEX service_specs_current ON service_specs (service_key) WHERE current = 1;
CREATE UNIQUE INDEX service_specs_current_name ON service_specs (name_folded) WHERE current = 1;

-- Les secrets d'une révision, scellés valeur par valeur. Supprimés avec leur révision.
CREATE TABLE service_spec_secrets (
    service_key TEXT NOT NULL,
    rev         INTEGER NOT NULL,
    -- `env:<rang>` (nom ET valeur, scellés ensemble), `command`, `answer:<identifiant de question>`.
    field       TEXT NOT NULL,
    -- Version du format de scellement (algorithme, composition des données associées).
    format      INTEGER NOT NULL DEFAULT 1 CHECK (format >= 1),
    -- 96 bits tirés du hasard du système pour CE scellement, jamais réutilisés.
    nonce       BLOB NOT NULL CHECK (length(nonce) = 12),
    -- Texte chiffré suivi de l'étiquette d'authentification (16 octets).
    sealed      BLOB NOT NULL CHECK (length(sealed) >= 16),
    PRIMARY KEY (service_key, rev, field),
    FOREIGN KEY (service_key, rev) REFERENCES service_specs (service_key, rev) ON DELETE CASCADE
) STRICT;

-- Ceinture du « nonce jamais réutilisé » : un tirage identique à un autre ferait échouer l'écriture.
CREATE UNIQUE INDEX service_spec_secrets_nonce ON service_spec_secrets (nonce);

-- Une opération : démarrer, déployer, modifier, supprimer, sauvegarder, restaurer, reprendre un monde.
CREATE TABLE service_ops (
    id             TEXT NOT NULL PRIMARY KEY,
    -- Clé d'opération du client (rejeu après coupure, ADR-0034).
    op_key         TEXT NOT NULL,
    service_key    TEXT NOT NULL,
    -- Nom du service, figé à l'acceptation.
    service_name   TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN
        ('start', 'stop', 'restart', 'deploy', 'update', 'delete', 'resources', 'backup', 'restore',
         'world_import')),
    account        TEXT NOT NULL,
    -- Le plan figé à l'acceptation : les étapes, lesquelles sont destructives, le point de non-retour.
    -- JAMAIS un secret.
    plan_json      TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN
        ('accepted', 'running', 'awaiting_decision', 'cancelling', 'cancelled', 'rolling_back',
         'rolled_back', 'succeeded', 'failed', 'interrupted', 'needs_attention')),
    -- L'étape en cours (code stable), nulle avant la première.
    step           TEXT,
    -- Les choix offerts au compte, nuls sinon.
    decision_json  TEXT,
    -- Depuis quand la décision attend (elle expire en 15 minutes).
    decision_at    TEXT,
    -- Compteurs de progression (pourcentage, éléments, octets).
    progress_json  TEXT NOT NULL DEFAULT '{}',
    -- Code de la liste fermée des causes, ou vide.
    cause          TEXT NOT NULL DEFAULT '',
    forced         INTEGER NOT NULL DEFAULT 0 CHECK (forced IN (0, 1)),
    started_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    finished_at    TEXT
) STRICT;

-- Ceinture du verrou exclusif : tous les genres sont exclusifs sur leur service, donc deux opérations
-- non terminées ne partagent jamais un service, même si le verrou en mémoire avait un défaut.
CREATE UNIQUE INDEX service_ops_open ON service_ops (service_key)
    WHERE status IN ('accepted', 'running', 'awaiting_decision', 'cancelling', 'rolling_back', 'needs_attention');
CREATE INDEX service_ops_op_key ON service_ops (op_key);
CREATE INDEX service_ops_finished_at ON service_ops (finished_at);

-- Le journal d'intention : une ligne par changement d'état d'une étape. `intent` est validée sur le
-- disque AVANT le geste destructif, `done` après. Ce que l'opération a créé pour elle-même et ce qu'il
-- faut pour défaire sont dans `undo_json`, sans secret.
CREATE TABLE service_op_steps (
    op_id     TEXT NOT NULL REFERENCES service_ops (id) ON DELETE CASCADE,
    seq       INTEGER NOT NULL CHECK (seq >= 1),
    step      TEXT NOT NULL,
    state     TEXT NOT NULL CHECK (state IN ('intent', 'done', 'failed', 'undone')),
    undo_json TEXT NOT NULL DEFAULT '{}',
    at        TEXT NOT NULL,
    PRIMARY KEY (op_id, seq)
) STRICT;
