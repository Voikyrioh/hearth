# VariantChoice

Molécule · `apps/desktop/src/components/molecules/VariantChoice.vue`

Le choix de la variante d'une fiche (HRT-102, BR-FICHE-060) : cartes exclusives (boutons radio natifs) jusqu'à quatre variantes, liste au-delà, chacune avec le libellé et l'aide écrits par la fiche. La variante proposée est présélectionnée ; « Ne pourra plus être changée : il faudrait installer un autre serveur. » suit le choix.

- Props : `variants` (`FicheDetail['variants']`), `modelValue` (l'identifiant choisi)
- Événements et slots : `update:modelValue`
- Notes : Posé par `InstallAnswers` seulement quand la fiche a plusieurs variantes. Tests : `pages/InstallFromFiche.test.ts`.
