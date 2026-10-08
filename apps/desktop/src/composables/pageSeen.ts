import { readonly, ref } from "vue";

// Une page qui n'a encore RIEN reçu le dit ici : le gabarit (`StaleSurface`) ne pose alors pas « Vu il y a… »
// sur une page qui n'a rien vu (revue de la PR #57, tableau de bord sans mesure). Un seul indicateur : la page
// affichée.
const nothingSeen = ref(false);

export const pageNothingSeen = readonly(nothingSeen);

export function setPageNothingSeen(value: boolean): void {
  nothingSeen.value = value;
}
