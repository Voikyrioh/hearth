-- Avis de fermeture des sessions (HRT-133, BR-ACCT-019) : quand un AUTRE compte ferme les sessions d'un
-- compte par le réseau, le compte visé le lit une fois, à sa connexion suivante : « Tes sessions ont été
-- fermées par {identifiant} le {date}. ». Une ligne par compte au plus (une fermeture plus récente
-- remplace l'ancienne) ; la ligne est supprimée dans la transaction de la connexion qui la livre.
-- Aucune adresse, aucun poste, aucun secret : l'identifiant de l'administrateur et la date seulement.
-- Même convention de dates que 0001 (texte UTC à largeur fixe). Migration additive : une table neuve,
-- rien d'existant n'est relu ni réécrit.
CREATE TABLE session_closure_notices (
    account_id TEXT NOT NULL PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    closed_by  TEXT NOT NULL,
    closed_at  TEXT NOT NULL
) STRICT;
