/**
 * LA règle « modifié » (HRT-109, BR-FICHE-041, BR-FICHE-040) : la valeur actuelle diffère de la valeur proposée. Une
 * feuille sans import : l'onglet (`settings/edits.ts`) et le récapitulatif de l'installation (`fiche/answers.ts`) la
 * partagent, ils ne la recopient pas. Sans valeur proposée, « non écrit » est la valeur proposée : tout réglage écrit est
 * modifié. Un secret défini (ou saisi) est modifié ; sa valeur n'est jamais comparée.
 */
export function differsFromProposed(
  proposed: string | null,
  secret: boolean,
  current: { written: boolean; value: string | null },
): boolean {
  if (secret || proposed === null) return current.written;
  return !(current.written && current.value === proposed);
}
