import type { FicheDetail } from "./catalog";
import type { DeployRefusal } from "./deploy";
import type { PackCheck, PackDeposit } from "./packs";

/**
 * La zone d'attente des packs de mods du pont SIMULÉ (HRT-124) : un agent d'essai qui juge un pack COMME LE VRAI
 * (`crates/hearth-agent/src/application/pack_intake.rs`, `deployments.rs`) : un dépôt inconnu se dit `PACK_UNKNOWN`, un pack
 * refusé est retiré AUSSITÔT (BR-MOD-016), la fiche doit offrir le genre lu avec la variante choisie. Un test dit à l'avance
 * ce que le « choix du fichier » dépose (`willDeposit`) : le contenu d'un fichier n'existe pas dans un navigateur d'essai,
 * seulement ce que l'agent en aurait lu. Jamais livré (derrière `import.meta.env.DEV`).
 */
export class SimulatedPacks {
  private readonly deposits = new Map<string, PackCheck>();
  private readonly upcoming: PackDeposit[] = [];
  /** Les dépôts que l'agent d'essai a retirés (pack refusé), dans l'ordre. */
  readonly removed: string[] = [];

  /** Un fichier déjà dans la zone d'attente, avec ce que l'agent en lit. */
  put(deposit: string, check: PackCheck): void {
    this.deposits.set(deposit, check);
  }

  /** Le prochain « choix du fichier » dépose ce fichier. */
  willDeposit(deposit: string, fileName: string, check: PackCheck): void {
    this.put(deposit, check);
    this.upcoming.push({ kind: "deposited", deposit, fileName });
  }

  /** Le prochain « choix du fichier » est annulé par l'utilisateur. */
  willCancel(): void {
    this.upcoming.push({ kind: "cancelled" });
  }

  nextDeposit(): PackDeposit {
    return this.upcoming.shift() ?? { kind: "unavailable" };
  }

  /** `POST /deployments/pack-check` : une consultation ; un refus retire le dépôt. */
  inspect(deposit: string): PackCheck {
    const found = this.deposits.get(deposit);
    if (found === undefined) return { kind: "refused", refusal: { kind: "unknown" } };
    if (found.kind === "refused") this.remove(deposit);
    return structuredClone(found);
  }

  /** Le jugement d'une installation qui porte un pack, ou `null` : rien à dire. */
  judgeForInstall(
    deposit: string,
    detail: FicheDetail,
    variant: string | null,
  ): DeployRefusal | null {
    const found = this.deposits.get(deposit);
    if (found === undefined) return { kind: "pack_unknown" };
    if (found.kind === "refused") {
      this.remove(deposit);
      return found.refusal.kind === "unknown"
        ? { kind: "pack_unknown" }
        : {
            kind: "pack_refused",
            reason: found.refusal.reason,
            mods: found.refusal.mods,
            host: found.refusal.host,
          };
    }
    const genre = detail.mods?.packGenres.find((item) => item.kind === found.pack.kind);
    if (genre === undefined) return { kind: "invalid", field: "pack", reason: "not_offered" };
    if (variant === null || !genre.variants.includes(variant)) {
      return { kind: "invalid", field: "pack", reason: "variant_mismatch" };
    }
    return null;
  }

  private remove(deposit: string): void {
    this.deposits.delete(deposit);
    this.removed.push(deposit);
  }
}
