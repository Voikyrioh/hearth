import type { AuditEntry } from "@/link";
import { type Burst, groupBursts } from "./grouping";

/**
 * Les lignes du tableau, à plat (une hauteur fixe par ligne permet de ne dessiner que les lignes
 * visibles) : une entrée, ou une rafale regroupée suivie, quand elle est déployée, de chacune de ses
 * tentatives. Pur : l'ordre est celui des entrées, jamais modifié.
 *
 * L'état « déployé » se retient par identifiants d'entrées (`expanded`) : une rafale est déployée si
 * l'un de ses membres l'est, ce qui survit à l'arrivée d'une entrée en tête ou d'une page sous elle
 * (les deux changent la clé et la composition du groupe).
 */
export type DisplayRow =
  | { type: "entry"; key: string; entry: AuditEntry; child: boolean }
  | { type: "burst"; key: string; burst: Burst; expanded: boolean };

export function displayRows(
  entries: readonly AuditEntry[],
  expanded: ReadonlySet<number>,
  complete = true,
): DisplayRow[] {
  const rows: DisplayRow[] = [];
  for (const item of groupBursts(entries, complete)) {
    if (item.kind === "entry") {
      rows.push({ type: "entry", key: `e-${item.entry.id}`, entry: item.entry, child: false });
      continue;
    }
    const open = item.entries.some((entry) => expanded.has(entry.id));
    rows.push({ type: "burst", key: item.key, burst: item, expanded: open });
    if (open) {
      for (const entry of item.entries) {
        rows.push({ type: "entry", key: `e-${entry.id}`, entry, child: true });
      }
    }
  }
  return rows;
}
