-- Empreintes des requêtes suivies (HRT-32, ADR-0033, FIX-01M4BZN31A8Z8WN0WKNTCRTFFN).
--
-- Jusqu'ici `operations.request_hash` était un SHA-256 sans clé de la méthode, du chemin et du corps
-- de la requête. Le corps de `POST /accounts`, `PUT /accounts/{id}/password` et `PUT /me/password`
-- contient des mots de passe : qui lisait la base pouvait les deviner hors ligne à pleine vitesse.
-- L'empreinte est désormais un HMAC-SHA-256 clé par un secret du dossier de données (hors base).
--
-- Les anciennes empreintes sont effacées (chaîne vide), les lignes restent : le résultat d'une
-- opération reste lisible par `GET /operations/{id}`. Une clé dont l'empreinte est vide n'est égale à
-- aucune requête : l'agent répond `409 CONFLICT` et n'exécute RIEN (jamais de double exécution).
-- Conséquence, au plus 24 heures (durée de conservation) : un rejeu d'une opération faite avant la
-- mise à jour n'est plus reconnu comme tel ; le client relit son état au lieu de recevoir le premier
-- résultat. Un retour arrière remet la base d'avant l'échange (BR-UPDATE-029) avec ses anciennes
-- empreintes, que l'ancien binaire sait lire.

UPDATE operations SET request_hash = '';
