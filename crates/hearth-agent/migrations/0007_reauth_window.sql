-- Fréquence du mot de passe en administration (HRT-28, Q19, BR-TRUST-042) : réglage par compte, tenu par
-- l'agent. 300 : une saisie du mot de passe ouvre une élévation de 5 minutes (défaut) ; 0 : le mot de
-- passe est demandé à chaque acte. Migration additive : aucune donnée n'est réécrite, les comptes
-- existants prennent le défaut. Un ancien binaire ignore la colonne (le superviseur remet la copie de la
-- base d'avant l'échange s'il doit revenir, BR-UPDATE-029).

ALTER TABLE accounts ADD COLUMN reauth_window_s INTEGER NOT NULL DEFAULT 300 CHECK (reauth_window_s IN (0, 300));
