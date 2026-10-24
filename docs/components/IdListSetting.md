# IdListSetting

Molécule · `apps/desktop/src/components/molecules/IdListSetting.vue`

Une liste d'identifiants (HRT-109, BR-FICHE-037) : un champ multiligne à chasse fixe, un identifiant par ligne, le compteur « {n} sur {max} », et toujours « Le jeu lit lui-même ces identifiants. Hearth ne les vérifie pas. ». Séparateur, jeu de caractères et maximum sont ceux de la fiche.

- Props : `setting` (`FicheSetting`), `modelValue` (la valeur que le jeu lit), `disabled`
- Événements et slots : `update:modelValue`
- Notes : Règles pures : `settings/edits.ts::{listIssue, listItems, listText, listValue}`.
