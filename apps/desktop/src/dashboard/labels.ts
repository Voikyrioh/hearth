import { t } from "@/i18n";

// HRT-142 : les cartes Disques et Températures parlent d'abord comme une personne, le nom interne du système
// reste en second plan. Une valeur qu'on ne reconnaît pas garde son nom tel que le serveur le donne.

export interface Titled {
  /** Ce qui se lit en premier. */
  title: string;
  /** Le nom du système, en second plan ; `null` quand le titre EST le nom du système. */
  technical: string | null;
}

/** Un disque : « Disque système » pour la racine, sinon son dossier ; le nom du périphérique reste en second plan. */
export function diskTitle(name: string, mount: string): Titled {
  if (mount === name || mount.trim() === "") return { title: name, technical: null };
  if (mount === "/") return { title: t("dash.diskSystem"), technical: name };
  return { title: mount, technical: name };
}

/** Une sonde de température connue porte un nom simple ; une sonde inconnue garde son nom. */
export function probeTitle(label: string): Titled {
  if (/^coretemp Package id \d+$/.test(label)) {
    return { title: t("dash.probeProcessor"), technical: label };
  }
  if (label === "nvme Composite") return { title: t("dash.probeSsd"), technical: label };
  return { title: label, technical: null };
}
