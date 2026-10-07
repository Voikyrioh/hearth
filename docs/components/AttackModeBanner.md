# AttackModeBanner

Organisme · `apps/desktop/src/components/organisms/AttackModeBanner.vue`

Bandeau permanent « Mode attaque actif » (HRT-26, BR-TRUST-010), posé par `ServerLayout` sur toutes les pages du serveur tant que le mode est actif ou suspendu, hors de la surface des données périmées, non fermable. Le sens est écrit en clair : « Seuls les postes reconnus peuvent se connecter. Un poste connu par un seul signe a droit à un essai. ». Variante suspendue : « Mode attaque suspendu », « Le serveur a redémarré. Le mode attaque reprend dans {N} min. » (« moins d'une minute » sous la minute, sans zone live). Aucun bouton de désactivation ici : on désactive depuis la page Sécurité (« Voir la page Sécurité », absent sur elle).

- Props : `suspended`, `minutes`, `stamp`, `onPage`
- Événements et slots : `open`
- Notes : Tests : `pages/SecurityMode.test.ts`, `e2e/attack-mode.spec.ts`.
