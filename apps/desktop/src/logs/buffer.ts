import type { LogLine, LogSource } from "@/link";
import { displayText } from "./text";

/**
 * Le tampon des lignes affichées (HRT-67, BR-SLOG-006, BR-SLOG-007). Un objet ordinaire, NON réactif : à 1 000 lignes
 * par seconde, une réactivité par ligne figerait l'interface. La vue ne relit que la fenêtre visible, sur un signal
 * unique (`version` du magasin). La mémoire est bornée en lignes ET en caractères : au-delà, les plus anciennes entrées
 * sont retirées d'un coup et le retrait est compté (`removed`) pour que l'écran le dise.
 */

/** Lignes gardées au plus (BR-SLOG-006). */
export const MAX_LINES = 5_000;
/** Caractères gardés au plus, toutes lignes confondues (le plafond de 2 Mio de l'agent, en caractères). */
export const MAX_CHARS = 2_000_000;
/** Deux redémarrages à moins de cette durée se regroupent en un seul repère (« n fois »). */
export const BURST_MS = 1_000;

export type GapKind =
  /** Des lignes manquent : le moteur ne les a pas gardées (coupure trop longue, rotation...). */
  | "missing"
  /** Le client lisait trop lentement : l'agent a gardé les plus récentes et compté les autres (BR-SLOG-007). */
  | "slow"
  /** Des lignes sont arrivées pendant la pause et n'ont pas toutes été gardées (BR-SLOG-006). */
  | "pause";

export type Entry =
  | {
      id: number;
      kind: "line";
      t: number | null;
      source: LogSource;
      /** Texte déjà rendu inoffensif (`displayText`). */
      text: string;
      /** Nombre total de caractères d'une ligne que l'agent a coupée. */
      cut: number | null;
    }
  | { id: number; kind: "restart"; at: number | null; count: number }
  | { id: number; kind: "gap"; gap: GapKind; count: number | null };

/** Une entrée avant son numéro. */
export type NewEntry =
  | { kind: "line"; t: number | null; source: LogSource; text: string; cut: number | null }
  | { kind: "restart"; at: number | null }
  | { kind: "gap"; gap: GapKind; count: number | null };

/** Une ligne reçue, rendue inoffensive. */
export function lineEntry(line: LogLine): NewEntry {
  return {
    kind: "line",
    t: line.t,
    source: line.source,
    text: displayText(line.text),
    cut: line.cut,
  };
}

export class LogBuffer {
  entries: Entry[] = [];
  private chars = 0;
  private next = 1;
  /** Lignes retirées par le haut depuis l'ouverture (remis à zéro par `clear`). */
  removed = 0;

  /** Ajoute ces entrées, puis retire les plus anciennes au-delà des plafonds. Rend le nombre de lignes retirées. */
  append(items: readonly NewEntry[], trim = true): number {
    for (const item of items) {
      if (item.kind === "restart") {
        const last = this.entries[this.entries.length - 1];
        // Une rafale de redémarrages (plus d'un par seconde) est UN repère « n fois ».
        if (
          last?.kind === "restart" &&
          last.at !== null &&
          item.at !== null &&
          item.at - last.at <= BURST_MS
        ) {
          last.count += 1;
          continue;
        }
        this.entries.push({ id: this.next++, kind: "restart", at: item.at, count: 1 });
        continue;
      }
      if (item.kind === "line") {
        this.chars += item.text.length;
        this.entries.push({ ...item, id: this.next++ });
        continue;
      }
      this.entries.push({ id: this.next++, kind: "gap", gap: item.gap, count: item.count });
    }
    return trim ? this.trim() : 0;
  }

  /** Retire les plus anciennes entrées au-delà de `MAX_LINES` lignes ou `MAX_CHARS` caractères. */
  trim(): number {
    let lines = 0;
    for (const entry of this.entries) if (entry.kind === "line") lines += 1;
    let drop = 0;
    let dropped = 0;
    while (drop < this.entries.length && (lines > MAX_LINES || this.chars > MAX_CHARS)) {
      const entry = this.entries[drop];
      if (!entry) break;
      if (entry.kind === "line") {
        lines -= 1;
        this.chars -= entry.text.length;
        dropped += 1;
      }
      drop += 1;
    }
    if (drop > 0) {
      this.entries.splice(0, drop);
      this.removed += dropped;
    }
    return dropped;
  }

  /**
   * Après un `resume_lost` : la page qui suit répète les dernières lignes du moteur. Retire de la fin les lignes
   * dont l'heure est au moins `from` (elles reviennent) ; rend combien. Le repère de trou posé juste avant (`gapId`)
   * est retiré s'il y a eu recouvrement : aucune ligne ne manque alors.
   */
  dropOverlap(from: number, gapId: number | null): number {
    let overlap = 0;
    let index = this.entries.length;
    while (index > 0) {
      const entry = this.entries[index - 1];
      if (!entry) break;
      if (entry.kind === "gap" && entry.id === gapId) {
        index -= 1;
        continue;
      }
      if (entry.kind === "line" && entry.t !== null && entry.t >= from) {
        overlap += 1;
        index -= 1;
        continue;
      }
      break;
    }
    if (overlap > 0) {
      const tail = this.entries.slice(index);
      for (const entry of tail) if (entry.kind === "line") this.chars -= entry.text.length;
      this.entries = this.entries.slice(0, index);
    }
    return overlap;
  }

  /** Efface tout : les lignes ne sont plus nulle part. */
  clear(): void {
    this.entries = [];
    this.chars = 0;
    this.removed = 0;
  }

  /** L'identifiant de la dernière entrée si c'est un repère de trou, sinon `null`. */
  lastGapId(): number | null {
    const last = this.entries[this.entries.length - 1];
    return last?.kind === "gap" ? last.id : null;
  }
}
