import type { UpdateFailure, UpdateStateDto } from "@/bindings";
import type { Unsubscribe, UpdateBridge } from "./bridge";

const DAY_MS = 24 * 60 * 60 * 1000;

export type InstallOutcome = "ok" | UpdateFailure;

export interface SimulatedUpdateOptions {
  /** Version du client qui tourne. */
  currentVersion?: string;
  /** Horloge (ms) ; l'instant courant par défaut. */
  now?: () => number;
  /** Dernière vérification réussie déjà faite, en ms avant maintenant (`null` : jamais). */
  checkedAgoMs?: number | null;
  /** Une version plus récente est déjà connue (bandeau visible au départ). */
  available?: { version: string; notes: string } | null;
}

/**
 * Pont simulé : un petit modèle de la coquille, pilotable (développement, Vitest, Playwright).
 * Il rejoue les règles de la coquille (bandeau masqué 24 h par « Plus tard », échec silencieux de la
 * vérification sans Internet, installation qui peut être interrompue ou refusée) mais ne touche à
 * rien. Retiré du binaire livré (`scripts/check-dist.mjs`).
 */
export class SimulatedUpdateBridge implements UpdateBridge {
  private readonly listeners = new Set<(state: UpdateStateDto) => void>();
  private readonly now: () => number;
  private seq = 0;
  private phase: UpdateStateDto["phase"] = "idle";
  private progress: number | null = null;
  private failure: UpdateFailure | null = null;
  private lastAttemptAt: number | null;
  private lastSuccessAt: number | null;
  private postponedUntil: number | null = null;
  private available: { version: string; notes: string } | null;
  /** Ce que le flux de versions proposerait. */
  private feed: { version: string; notes: string } | null = null;
  private online = true;
  private outcome: InstallOutcome = "ok";
  /** Appels reçus, pour les assertions des tests. */
  readonly calls = { check: 0, postpone: 0, install: 0 };
  /** Combien de fois « l'installateur a été lancé » (jamais sans clic). */
  installed = 0;

  constructor(private readonly options: SimulatedUpdateOptions = {}) {
    this.now = options.now ?? Date.now;
    const checkedAgo =
      options.checkedAgoMs === undefined ? 2 * 60 * 60 * 1000 : options.checkedAgoMs;
    this.lastSuccessAt = checkedAgo === null ? null : this.now() - checkedAgo;
    this.lastAttemptAt = this.lastSuccessAt;
    this.available = options.available ?? null;
    this.feed = this.available;
  }

  // ---- pilotage (tests, développement) -----------------------------------------------------

  /** La version que le flux propose à la prochaine vérification (`null` : rien de plus récent). */
  setFeed(release: { version: string; notes: string } | null): void {
    this.feed = release;
  }
  /** Sans Internet : les vérifications échouent en silence, les téléchargements sont coupés. */
  setOnline(online: boolean): void {
    this.online = online;
  }
  /** Issue de la prochaine installation demandée. */
  setInstallOutcome(outcome: InstallOutcome): void {
    this.outcome = outcome;
  }
  /** Passe `ms` de temps : republie l'état si le report est échu. */
  advance(ms: number): void {
    this.shift += ms;
    this.publish();
  }
  private shift = 0;
  private clock(): number {
    return this.now() + this.shift;
  }

  /** Mène le téléchargement demandé à son terme (interrompu, refusé, ou installé). */
  finishInstall(): void {
    if (this.phase !== "downloading") return;
    if (this.outcome === "ok") {
      this.phase = "installing";
      this.progress = null;
      this.installed += 1;
    } else {
      this.phase = "idle";
      this.progress = null;
      this.failure = this.outcome;
    }
    this.publish();
  }
  /** Avancement du téléchargement en cours. */
  setProgress(percent: number): void {
    if (this.phase !== "downloading") return;
    this.progress = percent;
    this.publish();
  }

  // ---- le contrat ---------------------------------------------------------------------------

  async getState(): Promise<UpdateStateDto> {
    return this.snapshot();
  }

  async check(): Promise<UpdateStateDto> {
    this.calls.check += 1;
    if (this.phase !== "idle") return this.snapshot();
    const started = this.clock();
    this.lastAttemptAt = started;
    if (this.online) {
      this.lastSuccessAt = started;
      const newer =
        this.feed && isNewer(this.feed.version, this.currentVersion()) ? this.feed : null;
      if (newer?.version !== this.available?.version) this.failure = null;
      this.available = newer;
    }
    // Hors ligne : rien ne change, aucun message (BR-UPDATE-007).
    this.publish();
    return this.snapshot();
  }

  async postpone(): Promise<UpdateStateDto> {
    this.calls.postpone += 1;
    if (this.phase === "downloading" || this.phase === "installing") return this.snapshot();
    if (this.available) {
      this.postponedUntil = this.clock() + DAY_MS;
      this.failure = null;
      this.publish();
    }
    return this.snapshot();
  }

  async install(): Promise<UpdateStateDto> {
    this.calls.install += 1;
    if (this.phase === "idle" && this.available) {
      this.phase = "downloading";
      this.progress = 0;
      this.failure = null;
      this.publish();
    }
    return this.snapshot();
  }

  async onState(listener: (state: UpdateStateDto) => void): Promise<Unsubscribe> {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  // ---- interne ------------------------------------------------------------------------------

  private currentVersion(): string {
    return this.options.currentVersion ?? "0.1.0";
  }

  private postponed(): boolean {
    const now = this.clock();
    return this.postponedUntil !== null && now < this.postponedUntil;
  }

  private snapshot(): UpdateStateDto {
    return {
      seq: this.seq,
      currentVersion: this.currentVersion(),
      phase: this.phase,
      progress: this.progress,
      available: this.available ? { ...this.available } : null,
      bannerVisible: this.available !== null && !this.postponed(),
      postponedUntil: this.postponed() ? this.postponedUntil : null,
      lastCheckedAt: this.lastSuccessAt,
      upToDate:
        this.available === null &&
        this.lastSuccessAt !== null &&
        this.lastSuccessAt === this.lastAttemptAt,
      failure: this.failure,
    };
  }

  private publish(): void {
    this.seq += 1;
    const state = this.snapshot();
    for (const listener of this.listeners) listener(state);
  }
}

/** Plus récente au sens des numéros `x.y.z` (suffisant pour la simulation). */
function isNewer(candidate: string, current: string): boolean {
  const a = candidate.split(".").map(Number);
  const b = current.split(".").map(Number);
  for (let i = 0; i < 3; i += 1) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}
