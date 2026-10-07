-- Verrouillage qui résiste au changement d'adresse (HRT-20, ADR-0022). Même convention de dates
-- que 0001 (texte UTC à largeur fixe). Migration additive : deux tables nouvelles, rien de modifié.

-- Adresses connues d'un compte : adresse EXACTE (en IPv6 l'adresse complète, jamais le préfixe)
-- d'une connexion réussie de ce compte (BR-CONN-019). Au plus 8 par compte (borne tenue par le
-- domaine), valables 30 jours après la dernière connexion réussie. Supprimées avec le compte.
CREATE TABLE known_addresses (
    account_id      TEXT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    address         TEXT NOT NULL,
    last_success_at TEXT NOT NULL,
    PRIMARY KEY (account_id, address)
) STRICT;

CREATE INDEX known_addresses_address ON known_addresses (address);
CREATE INDEX known_addresses_last_success_at ON known_addresses (last_success_at);

-- Ralentissement par identifiant (BR-CONN-018) : la clé est l'empreinte de l'identifiant saisi
-- (jamais l'identifiant en clair), qu'il existe ou non. Bornée à 10 000 lignes par le domaine ;
-- l'index sert à oublier d'abord les identifiants les moins attaqués, les plus anciens.
CREATE TABLE identifier_slowdowns (
    key             TEXT NOT NULL PRIMARY KEY,
    failures        INTEGER NOT NULL,
    wait_until      TEXT,
    last_failure_at TEXT NOT NULL
) STRICT;

CREATE INDEX identifier_slowdowns_rank ON identifier_slowdowns (failures, last_failure_at);

-- Reprise : les adresses des sessions encore valides sont des adresses connues (une session
-- vient d'une connexion réussie). L'administrateur ne perd pas son poste habituel à la mise à
-- jour de l'agent. Au plus 8 par compte, les plus récentes.
INSERT INTO known_addresses (account_id, address, last_success_at)
SELECT account_id, client_addr, last_success_at
FROM (
    SELECT account_id,
           client_addr,
           MAX(created_at) AS last_success_at,
           ROW_NUMBER() OVER (PARTITION BY account_id ORDER BY MAX(created_at) DESC) AS nth
    FROM sessions
    WHERE expires_at > strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
    GROUP BY account_id, client_addr
)
WHERE nth <= 8;
