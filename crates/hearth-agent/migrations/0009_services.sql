-- Services (HRT-63, ADR-0037 et ADR-0038) : toutes les tables du lot « déploiements », créées d'un
-- coup (conception technique, section 6). Même convention de dates que 0001 (texte UTC à largeur
-- fixe). Migration additive : quatre tables nouvelles et une colonne nouvelle, rien d'existant n'est
-- modifié ni relu.
--
-- Cette tranche n'écrit que `supervised_services` : les trois autres tables attendent leurs
-- tranches (actes en HRT-64, déploiements en HRT-68). Aucune colonne ne reçoit une valeur de
-- variable d'environnement, un nom de variable ni la commande d'un service : ils ne vivent qu'en
-- mémoire (types `Secret` et `Hidden`), puis dans le moteur de conteneurs.

-- Arrêt voulu depuis Hearth : écrit AVANT l'ordre d'arrêt, effacé quand Hearth démarre le service ou
-- quand il est revu en marche. Sert à distinguer « Arrêté » de « En erreur » (BR-SVC-005), même après
-- un redémarrage de l'agent.
CREATE TABLE service_intents (
    service_key  TEXT NOT NULL PRIMARY KEY,
    -- Vide pour un service supervisé (son conteneur est recréé par le système à chaque démarrage).
    container_id TEXT NOT NULL,
    intent       TEXT NOT NULL CHECK (intent IN ('stopped')),
    -- Identifiant du compte, figé : le compte peut disparaître ensuite.
    account      TEXT NOT NULL,
    at           TEXT NOT NULL
) STRICT;

-- Démarrer, arrêter, redémarrer : une ligne par acte. Purgée 24 h après sa fin. Au démarrage de
-- l'agent, toute ligne `running` passe à `interrupted` (aucune exécution ne survit à un arrêt).
CREATE TABLE service_actions (
    id           TEXT NOT NULL PRIMARY KEY,
    -- Clé d'opération du client (rejeu après coupure).
    op_key       TEXT NOT NULL,
    service_key  TEXT NOT NULL,
    -- Nom du service, figé à l'acte.
    service_name TEXT NOT NULL,
    origin       TEXT NOT NULL CHECK (origin IN ('hearth', 'external')),
    kind         TEXT NOT NULL CHECK (kind IN ('start', 'stop', 'restart')),
    account      TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'interrupted')),
    -- Code de la liste fermée des causes, ou vide.
    cause        TEXT NOT NULL DEFAULT '',
    forced       INTEGER NOT NULL DEFAULT 0 CHECK (forced IN (0, 1)),
    started_at   TEXT NOT NULL,
    finished_at  TEXT
) STRICT;

CREATE INDEX service_actions_service ON service_actions (service_key, started_at);
CREATE INDEX service_actions_finished_at ON service_actions (finished_at);

-- Déploiements d'une image : une ligne par déploiement. Purgée 24 h après sa fin.
CREATE TABLE deployments (
    id                  TEXT NOT NULL PRIMARY KEY,
    op_key              TEXT NOT NULL,
    -- `h_<ULID>`, réservée à l'acceptation.
    service_key         TEXT NOT NULL,
    name                TEXT NOT NULL,
    -- Nom en minuscules : deux déploiements du même nom ne s'acceptent pas ensemble.
    name_folded         TEXT NOT NULL,
    image               TEXT NOT NULL,
    tag                 TEXT NOT NULL,
    -- Ports du serveur et protocoles ; chemins hors racine Hearth. Aucune valeur secrète.
    ports_json          TEXT NOT NULL DEFAULT '[]',
    external_paths_json TEXT NOT NULL DEFAULT '[]',
    env_count           INTEGER NOT NULL DEFAULT 0,
    custom_command      INTEGER NOT NULL DEFAULT 0 CHECK (custom_command IN (0, 1)),
    account             TEXT NOT NULL,
    step                TEXT NOT NULL CHECK (step IN ('verification', 'pull', 'create', 'start', 'watch')),
    -- Nul si la progression est inconnue.
    percent             INTEGER,
    status              TEXT NOT NULL CHECK (status IN ('running', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    cause               TEXT NOT NULL DEFAULT '',
    -- Nul avant la création du conteneur.
    container_id        TEXT,
    created_at          TEXT NOT NULL,
    finished_at         TEXT
) STRICT;

-- Garde de dernier recours : même si le verrou en mémoire avait un défaut, deux déploiements du même
-- nom ne peuvent pas être acceptés ensemble.
CREATE UNIQUE INDEX deployments_running_name ON deployments (name_folded) WHERE status = 'running';
CREATE INDEX deployments_finished_at ON deployments (finished_at);

-- Services gérés par une unité systemd : le service reste listé quand son conteneur n'existe plus
-- (arrêté ou en échec d'après l'unité), avec ses derniers réglages connus. La clé est `u_<unité>`.
-- `last_view_json` ne contient NI valeur de variable, NI nom de variable, NI commande.
CREATE TABLE supervised_services (
    service_key    TEXT NOT NULL PRIMARY KEY CHECK (substr(service_key, 1, 2) = 'u_'),
    unit           TEXT NOT NULL UNIQUE CHECK (service_key = 'u_' || unit),
    name           TEXT NOT NULL,
    last_view_json TEXT NOT NULL,
    last_seen_at   TEXT NOT NULL
) STRICT;

-- Journal d'activité : un objet typé rendu en texte par le domaine, jamais un champ libre. Nul pour
-- toutes les entrées existantes et pour celles qui n'ont rien à ajouter. Les entrées restent
-- immuables (le déclencheur `audit_events_no_update` ne se déclenche pas à l'ajout d'une colonne).
ALTER TABLE audit_events ADD COLUMN details TEXT;
