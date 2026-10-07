# PasswordDialog

Organisme · `apps/desktop/src/components/organisms/PasswordDialog.vue`

Changement de mot de passe : le sien (`account` absent : « Ancien mot de passe », « Nouveau mot de passe », confirmation ; ferme les AUTRES sessions, garde la courante) ou celui d'un autre compte (`account` fourni : nouveau + confirmation ; ferme toutes les sessions de ce compte). `username` : celui du compte dont le mot de passe change (règle « ne contient pas l'identifiant »). « L'ancien mot de passe est incorrect » sous le champ ; les champs sont vidés après chaque envoi.

- Props : `open`, `serverId`, `username`, `own` (mode explicite), `account`
- Événements et slots : Événement `close`
- Notes : BR-ACCT-008, BR-ACCT-009. Test : `accounts.test.ts`.
- HRT-26 (Q15) : pour SON mot de passe, la case « Garder ce poste reconnu » (décochée par défaut, aide « Décoché, il l'oublie avec celles des autres postes. »), désactivée avec sa raison quand l'agent est trop ancien, et un avertissement quand le mode attaque est actif sur un poste sans clé enregistrée (ne pas le garder refuse la session tout de suite). Elle envoie `keepAddress`. Test : `pages/SecuritySettings.test.ts`.
