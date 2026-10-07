# SecurityAlertBanner

Organisme · `apps/desktop/src/components/organisms/SecurityAlertBanner.vue`

Bandeau « Attaque probable détectée » (HRT-26, BR-TRUST-008, 009), posé par `ServerLayout` sur toutes les pages du serveur, hors de la surface des données périmées, non fermable. Le texte suit le rôle : le titulaire lit « Une attaque probable vise ton identifiant. Clique pour plus d'infos et activer le mode attaque. » ; un administrateur lit en plus le NOMBRE d'autres comptes visés (« … ton identifiant et 2 autres comptes », jamais un nom) ; un compte Lecture seule lit « Tu vois l'alerte, mais seul un administrateur peut activer le mode attaque. ». Actions : « Activer le mode attaque » (ouvre la confirmation avec mot de passe ; grisé et focusable avec sa raison en infobulle : Lecture seule, agent trop ancien, poste sans clé ; absent quand le mode est déjà actif), « Plus d'infos » (vers la page Sécurité, absent sur elle).

- Props : `alert`, `role`, `block`, `stamp`, `onPage`, `modeOn`
- Événements et slots : `activate`, `details`
- Notes : BR-TRUST-029 pour Lecture seule. Tests : `pages/SecurityMode.test.ts`, `e2e/attack-mode.spec.ts`.
