import type {
  DeviceRemovalOutcome,
  DeviceRemovalRefusal,
  TrustedDevice,
  TrustedDevices,
} from "./devices";
import type { ServerInfo } from "./types";

/**
 * Postes de confiance du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit
 * agent qui applique les mêmes règles que le vrai (huit postes au plus, le poste courant ne se retire
 * pas depuis lui-même, mot de passe ET clé de ce PC exigés, agent d'avant la clé d'appareil). Jamais
 * livré (derrière `import.meta.env.DEV`). Aucune clé n'existe ici : seul le fait « ce PC a une clé
 * inscrite » (`current`) est simulé, comme le voit l'interface.
 */
export const DEFAULT_PASSWORD = "Correct-Horse-9";

interface Book {
  devices: TrustedDevice[];
  max: number;
  /** Faux : agent d'avant la clé d'appareil (la liste dit `unsupported`). */
  supported: boolean;
}

export class SimulatedDevices {
  private readonly books = new Map<string, Book>();
  private nextId = 1;

  constructor(private readonly now: () => number) {}

  private newId(): string {
    const n = this.nextId++;
    return `01SIMDEVICE${String(n).padStart(15, "0")}`;
  }

  private book(server: ServerInfo): Book {
    let book = this.books.get(server.id);
    if (!book) {
      const day = 86_400_000;
      const at = (ago: number) => new Date(this.now() - ago).toISOString();
      book = {
        max: 8,
        supported: true,
        devices: [
          {
            id: this.newId(),
            name: "salon/0.1.0",
            createdAt: at(30 * day),
            lastProvedAt: at(2 * 60_000),
            lastAddr: "192.168.1.20",
            current: true,
          },
          {
            id: this.newId(),
            name: "bureau/0.1.0",
            createdAt: at(12 * day),
            lastProvedAt: at(3 * day),
            lastAddr: "192.168.1.31",
            current: false,
          },
        ],
      };
      this.books.set(server.id, book);
    }
    return book;
  }

  /** Remplace les postes d'un serveur (amorçage des tests). `max` : huit par défaut. */
  seed(
    server: ServerInfo,
    devices: Array<Partial<TrustedDevice> & { name: string }>,
    options: { max?: number; supported?: boolean } = {},
  ): TrustedDevice[] {
    const at = new Date(this.now()).toISOString();
    const book: Book = {
      max: options.max ?? 8,
      supported: options.supported ?? true,
      devices: devices.map((device) => ({
        id: device.id ?? this.newId(),
        name: device.name,
        createdAt: device.createdAt ?? at,
        lastProvedAt: device.lastProvedAt ?? at,
        lastAddr: device.lastAddr ?? "192.168.1.20",
        current: device.current ?? false,
      })),
    };
    this.books.set(server.id, book);
    return book.devices;
  }

  /** Les identifiants des postes, dans l'ordre de l'agent (tests). */
  ids(server: ServerInfo): string[] {
    return this.book(server).devices.map((device) => device.id);
  }

  list(server: ServerInfo): TrustedDevices {
    const book = this.book(server);
    if (!book.supported) return { kind: "unsupported" };
    return {
      kind: "listed",
      devices: book.devices.map((device) => ({ ...device })),
      max: book.max,
    };
  }

  /** Mêmes refus que l'agent, dans le même ordre ; le mot de passe n'est ni gardé ni noté. */
  remove(
    server: ServerInfo,
    deviceId: string,
    password: string,
    own: string,
  ): DeviceRemovalOutcome {
    const book = this.book(server);
    const refuse = (refusal: DeviceRemovalRefusal): DeviceRemovalOutcome => ({
      kind: "refused",
      refusal,
    });
    if (!book.supported) return refuse({ kind: "unsupported" });
    // Ce PC n'a pas de clé inscrite : aucun poste n'est « courant ».
    if (!book.devices.some((device) => device.current)) return refuse({ kind: "no_device_key" });
    const target = book.devices.find((device) => device.id === deviceId);
    if (target?.current) return refuse({ kind: "current_device" });
    if (!target) return refuse({ kind: "not_found" });
    if (password !== own) return refuse({ kind: "wrong_password" });
    book.devices = book.devices.filter((device) => device.id !== deviceId);
    return { kind: "done" };
  }
}
