-- Services adoptés (HRT-79, ADR-0043, BR-ADOPT-001, BR-ADOPT-017) : par défaut Hearth ne publie que
-- les services qu'il a créés (clé `h_`) ; un conteneur créé hors de Hearth n'est publié qu'une fois
-- adopté. Cette table est la mémoire de l'agent : une ligne par service adopté. L'adoption n'écrit
-- rien sur le service ni dans le moteur.
--
-- Ce ticket crée la table et la lit ; l'acte qui l'écrit (adopter, relâcher) est celui de HRT-80.
--
-- La clé est l'identité stable du service (ADR-0037) : `u_<unité>` pour un service lancé par une unité
-- du système (elle survit à la recréation du conteneur, au redémarrage de l'agent et à celui du
-- serveur), `c_<identifiant du conteneur>` sinon (l'adoption tombe avec le conteneur : le registre
-- efface la ligne quand le conteneur n'existe plus, jamais un homonyme ne l'hérite). Jamais une
-- adoption par nom. `h_` n'est jamais adopté : il est publié de droit.
CREATE TABLE adopted_services (
    service_key TEXT NOT NULL PRIMARY KEY
        CHECK (substr(service_key, 1, 2) IN ('u_', 'c_')),
    -- Nom du service au moment de l'adoption (journal d'activité, HRT-80) ; le nom publié reste celui
    -- que le moteur annonce.
    name        TEXT NOT NULL,
    -- Nom de l'unité du système, nul sans unité ; renseigné exactement quand la clé est `u_<unité>`.
    unit        TEXT,
    -- Identifiant du compte qui a adopté, figé : le compte peut disparaître ensuite.
    adopted_by  TEXT NOT NULL,
    -- Même convention de dates que 0001 (texte UTC à largeur fixe).
    adopted_at  TEXT NOT NULL,
    CHECK ((substr(service_key, 1, 2) = 'u_') = (unit IS NOT NULL)),
    CHECK (unit IS NULL OR service_key = 'u_' || unit)
) STRICT;

-- `supervised_services` (0009) retenait tout service lancé par une unité, adopté ou non, avec son nom,
-- son image, ses ports et ses dossiers : de quoi publier un conteneur que personne n'a choisi de faire
-- gérer. Elle ne garde désormais que les services adoptés. Au moment de cette migration aucun service
-- n'est encore adopté (la table vient d'être créée) : TOUTES les lignes existantes sont effacées. Les
-- services lancés par une unité se retiennent de nouveau, au fil des lectures, une fois adoptés.
DELETE FROM supervised_services
 WHERE service_key NOT IN (SELECT service_key FROM adopted_services);

-- Les deux sens de cet invariant sont tenus par la base elle-même, pas seulement par le registre :
-- on ne retient pas un service qui n'est pas adopté, et relâcher un service oublie ce qu'on en retenait.
CREATE TRIGGER supervised_services_only_adopted
BEFORE INSERT ON supervised_services
WHEN NOT EXISTS (SELECT 1 FROM adopted_services WHERE service_key = NEW.service_key)
BEGIN
    SELECT RAISE(ABORT, 'supervised_services ne retient que des services adoptés');
END;

CREATE TRIGGER adopted_services_release_forgets
AFTER DELETE ON adopted_services
BEGIN
    DELETE FROM supervised_services WHERE service_key = OLD.service_key;
END;
