# EnvVarsSection

Organisme · `apps/desktop/src/components/organisms/EnvVarsSection.vue`

Variables d'un service : noms visibles, valeurs masquées (8 puces fixes), révélées à la demande d'un compte qui a le droit (`reveal_service_env`, consigné par l'agent). Les valeurs vivent seulement dans `stores/serviceEnv.ts` (mémoire, jamais persisté), effacées au remasquage : fermeture, autre service, lien coupé, service disparu, droit retiré, moteur non prêt, démontage. Aucun bouton copier.

- Tests : `pages/ServicesDetail.test.ts`, `stores/serviceEnv.test.ts`.
