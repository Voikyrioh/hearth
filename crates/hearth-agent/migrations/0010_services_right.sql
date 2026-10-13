-- Le droit « gérer les services » (HRT-76, ADR-0041, BR-GRANT-001 et BR-GRANT-002) : une colonne du
-- compte, indépendante du rôle. Valeur par défaut `none` : AUCUN compte existant ne l'a après la
-- migration, pas même le premier administrateur, et un compte créé naît sans lui. Rien n'est accordé
-- en silence. Le droit se donne depuis le serveur seulement (ticket HRT-77) : aucune route n'écrit
-- cette colonne. Texte à deux valeurs aujourd'hui, pour qu'un découpage plus fin ne demande pas de
-- migration de forme. Migration additive : une colonne nouvelle, rien d'existant n'est relu.
ALTER TABLE accounts ADD COLUMN services_right TEXT NOT NULL DEFAULT 'none'
    CHECK (services_right IN ('none', 'manage'));
