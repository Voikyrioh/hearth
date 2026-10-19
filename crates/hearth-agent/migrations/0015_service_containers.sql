-- Les conteneurs que l'agent a CRÉÉS pour un service de Hearth (HRT-68, ADR-0037, BR-OPS-008).
--
-- L'étiquette `io.hearth.service-id` d'un conteneur peut être posée par n'importe qui, y compris par l'auteur
-- d'une image (`LABEL`) : elle ne vaut jamais identité. Un conteneur n'est « créé par Hearth » que si son
-- identifiant, rendu par le moteur au moment de la création, est consigné ici pour une clé `h_` connue de
-- l'agent. Une ligne par conteneur (deux pendant une recréation : l'ancien et le nouveau).
--
-- Aucune valeur de variable, aucun nom de variable, aucune commande : un identifiant de conteneur et une clé.
CREATE TABLE service_containers (
    container_id TEXT NOT NULL PRIMARY KEY,
    service_key  TEXT NOT NULL CHECK (service_key LIKE 'h\_%' ESCAPE '\'),
    -- Même convention de dates que 0001 (texte UTC à largeur fixe).
    recorded_at  TEXT NOT NULL
) STRICT;

CREATE INDEX service_containers_service ON service_containers (service_key);
