# ReauthSettingCard

Organisme · `apps/desktop/src/components/organisms/ReauthSettingCard.vue`

Ligne de réglage « Demander mon mot de passe » de la page Sécurité (HRT-30, D2, Q19) : « Toutes les 5 minutes » (défaut) ou « À chaque action » (`HSegmented`). La valeur est lue de l'agent ; la changer ouvre `AdminActDialog` (acte `reauth_setting`, jamais couvert par le délai : mot de passe et clé de ce PC). Absente face à un agent d'avant la confirmation des actes.

- Props : `serverId`
- Événements et slots : aucun
- Notes : Tests : `components/organisms/reauthSetting.test.ts`, `e2e/reauth.spec.ts`.
