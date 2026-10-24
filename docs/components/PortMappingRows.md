# PortMappingRows

Organisme · `apps/desktop/src/components/organisms/PortMappingRows.vue`

La section « Ports » du formulaire de déploiement (HRT-69, BR-DEPLOY-007 à 009, 031) : de 0 à 50 correspondances (`PortMappingRow`), compteur « n sur 50 », et dès qu'une rangée existe « Ce port sera accessible à tout appareil qui peut joindre votre serveur. ». Les constats de conflit (qui tient quoi, quel port libre proposer) arrivent de l'agent dans `verdicts` : la section n'en recalcule aucun. Sous 1024, l'avertissement est immédiat.

- Props : `rows`, `issues`, `verdicts`, `waiting`, `shown`
- Événements : `add`, `remove`, `update`, `protocol`, `suggest`, `touch`
- Notes : posée par `DeployForm`. Tests : `pages/DeployService.test.ts` (les ports).
