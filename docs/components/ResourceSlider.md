# ResourceSlider

Molécule · `apps/desktop/src/components/molecules/ResourceSlider.vue`

Une ressource à régler (HRT-146, BR-RES-004) : un curseur discret (`input[type=range]` sur l'indice du cran, `aria-valuetext` en clair : « 1,5 Go », « 2,5 cœurs »), ses extrémités, et à côté une saisie pour une valeur exacte (mémoire : nombre et liste Mo / Go ; processeur : nombre et « cœurs »). Virgule ou point acceptés. Le curseur se place sur le cran le plus proche, la saisie garde la valeur exacte. Une saisie vide ou refusée met la valeur à `null` et le dit sur place (« Indiquez la mémoire du service. », « La mémoire se règle entre 128 Mo et 64 Go. », « Le processeur se règle entre 0,25 et 16 cœurs, par pas de 0,25. »). Aucune case, position ni saisie ne retire la limite (BR-RES-037).

- Props : `kind` (`memory` | `cpu`), `stops` (crans permis, octets ou millicœurs), `modelValue` (`number | null`), `disabled`
- Événements et slots : `update:modelValue` ; slot `hint` (phrases sous le curseur)
- Notes : Ne connaît ni la machine ni la fiche : les crans viennent de `resources/scale.ts`. Tests : `components/organisms/resourceSliders.test.ts`, `resources/resources.test.ts`.
