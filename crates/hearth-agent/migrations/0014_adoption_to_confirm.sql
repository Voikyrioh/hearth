-- État « à confirmer » d'une adoption par unité, rendu durable (HRT-80, BR-ADOPT-027, revue de la PR 89).
--
-- Une adoption par unité (`u_<unité>`) dont l'unité n'est plus relue par l'agent n'ouvre plus aucun droit
-- tant qu'un compte qui a le droit ne l'a pas confirmée, et tombe au bout de 24 heures. Jusqu'ici cet état
-- et son échéance ne vivaient qu'en mémoire : un redémarrage de l'agent rechargeait toutes les adoptions
-- pleinement actives et le délai de 24 heures repartait. Cette colonne les tient en base :
--
-- - `NULL` : l'adoption n'est pas « à confirmer » (cas de toutes les lignes existantes) ;
-- - une date : l'adoption est « à confirmer » DEPUIS ce moment. Écrite avant que l'adoption cesse d'ouvrir
--   des droits, relue au démarrage, jamais réécrite tant que l'état dure : le délai ne repart donc jamais.
--
-- Confirmer ou relâcher se fait déjà en effaçant la ligne puis en la réécrivant (colonne à `NULL` par
-- défaut). Même convention de dates que 0001 (texte UTC à largeur fixe, 24 caractères). Seule une clé
-- `u_` peut être « à confirmer » : un conteneur sans unité (`c_`) tombe avec son conteneur (BR-ADOPT-017).
-- Migration additive : une colonne neuve, rien d'existant n'est relu ni réécrit.
ALTER TABLE adopted_services ADD COLUMN unconfirmed_since TEXT
    CHECK (
        unconfirmed_since IS NULL
        OR (length(unconfirmed_since) = 24 AND substr(service_key, 1, 2) = 'u_')
    );
