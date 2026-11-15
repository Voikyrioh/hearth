import type { DepositOutcome, Transfer, TransferRefusal } from "./transfers";

/**
 * Les transferts du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui juge une
 * demande COMME LE VRAI (`crates/hearth-agent/src/application/transfers.rs`, `docs/open-api/transfers.md`) : le
 * fichier doit être lisible, le disque du serveur peut manquer de place, une reprise est un geste. Rien n'avance
 * seul : l'appelant pilote les octets durables (`advance`), la coupure (`interrupt`), la fin (`complete`) et les
 * refus (`refuse`), comme `__hearthSim.deletes` pilote une suppression. Les chemins sont de faux chemins : un chemin
 * qui contient `introuvable` n'est pas lisible, un qui contient `gros` est trop gros pour le disque du serveur. Jamais
 * livré (derrière `import.meta.env.DEV`).
 */

const MIB = 1024 * 1024;

interface Held {
  transfer: Transfer;
  /** Annulé par un geste du compte pendant que l'appelant pilotait : plus rien ne s'avance. */
  cancelled: boolean;
}

/** Le dernier nom d'un chemin, avec ses deux séparateurs. */
export function nameOf(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

export class SimulatedTransfers {
  private readonly held = new Map<string, Held>();
  private counter = 0;
  /** Les gestes reçus, dans l'ordre (les tests les relisent). */
  readonly calls: string[] = [];
  /** La taille des fichiers « choisis » (les chemins sont faux) : 600 Mio par défaut. */
  fileSize = 600 * MIB;
  /** Le chemin que la boîte « Ouvrir » rend ; `null` : le compte annule la boîte. */
  picked: string | null = "C:\\Téléchargements\\Prominence_II_Server_Pack.zip";

  pick(): string | null {
    this.calls.push("pick");
    return this.picked;
  }

  list(): Transfer[] {
    return [...this.held.values()].map((entry) => ({ ...entry.transfer }));
  }

  async start(serverId: string, path: string): Promise<DepositOutcome> {
    this.calls.push(`start ${path}`);
    if (path.includes("introuvable")) {
      return { kind: "refused", refusal: { kind: "source_unreadable" } };
    }
    if (path.includes("gros")) {
      return {
        kind: "refused",
        refusal: { kind: "no_space", missing: 3 * 1024 * MIB, remaining: 512 * MIB },
      };
    }
    this.counter += 1;
    const id = `01J9ZY0G3Q8M2K6W4T7V5N1B${String(this.counter).padStart(2, "0")}`;
    const transfer: Transfer = {
      id,
      serverId,
      name: nameOf(path),
      size: this.fileSize,
      sent: 0,
      status: "sending",
      refusal: null,
    };
    this.held.set(id, { transfer, cancelled: false });
    return { kind: "started", transfer: { ...transfer } };
  }

  /** Reprend un envoi interrompu : un geste du compte, jamais automatique. */
  resume(id: string): void {
    this.calls.push(`resume ${id}`);
    const entry = this.held.get(id);
    if (entry && entry.transfer.status === "interrupted") {
      entry.transfer.status = "sending";
      entry.transfer.refusal = null;
    }
  }

  /** Annule : ferme l'envoi et retire la partie reçue du serveur. */
  cancel(id: string): void {
    this.calls.push(`cancel ${id}`);
    const entry = this.held.get(id);
    if (entry && (entry.transfer.status === "sending" || entry.transfer.status === "interrupted")) {
      entry.transfer.status = "cancelled";
      entry.transfer.refusal = null;
      entry.cancelled = true;
    }
  }

  /** Retire de la liste un transfert fini ; un transfert encore ouvert reste. */
  dismiss(id: string): void {
    this.calls.push(`dismiss ${id}`);
    const entry = this.held.get(id);
    if (entry && entry.transfer.status !== "sending" && entry.transfer.status !== "interrupted") {
      this.held.delete(id);
    }
  }

  // — Le pilotage par l'appelant ————————————————————————————————————————————————————————————————

  /** Des octets de plus sont durables sur le serveur (jamais en arrière, jamais au-delà du fichier). */
  advance(id: string, bytes: number): void {
    const entry = this.held.get(id);
    if (!entry || entry.transfer.status !== "sending") return;
    entry.transfer.sent = Math.min(entry.transfer.size, Math.max(entry.transfer.sent, bytes));
  }

  /** Le lien coupe : « Reprendre l'envoi » est un geste. */
  interrupt(id: string): void {
    const entry = this.held.get(id);
    if (entry && entry.transfer.status === "sending") entry.transfer.status = "interrupted";
  }

  /** Le fichier est dans la zone d'attente, vérifié. */
  complete(id: string): void {
    const entry = this.held.get(id);
    if (!entry || entry.transfer.status !== "sending") return;
    entry.transfer.sent = entry.transfer.size;
    entry.transfer.status = "done";
  }

  /** Le serveur refuse de continuer : pas de place ou trop de transferts laissent l'envoi reprenable. */
  refuse(id: string, refusal: TransferRefusal): void {
    const entry = this.held.get(id);
    if (!entry || entry.transfer.status !== "sending") return;
    const resumable = refusal.kind === "no_space" || refusal.kind === "too_many";
    entry.transfer.status = resumable ? "interrupted" : "failed";
    entry.transfer.refusal = refusal;
  }
}
