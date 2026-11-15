import { describe, expect, it } from "vitest";
import type { Transfer } from "@/link";
import {
  canCancel,
  canDismiss,
  canResume,
  depositRefusalText,
  formatSize,
  isOpen,
  progressText,
  ratioOf,
  refusalText,
  statusText,
} from "./messages";

// Les règles et les textes du panneau « Transferts » (HRT-96, BR-FILES-013, BR-FILES-019) : des fonctions pures.
const MIB = 1024 * 1024;
const GIB = 1024 * MIB;

function transfer(patch: Partial<Transfer> = {}): Transfer {
  return {
    id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
    serverId: "forge",
    name: "Prominence_II_Server_Pack.zip",
    size: 600 * MIB,
    sent: 150 * MIB,
    status: "sending",
    refusal: null,
    ...patch,
  };
}

describe("les tailles", () => {
  it("se disent en octets binaires, avec une décimale seulement sous 10", () => {
    expect(formatSize(0)).toBe("0 o");
    expect(formatSize(900)).toBe("900 o");
    expect(formatSize(2048)).toBe("2 Kio");
    expect(formatSize(600 * MIB)).toBe("600 Mio");
    expect(formatSize(1.5 * GIB)).toBe("1,5 Gio");
    expect(formatSize(12.4 * GIB)).toBe("12 Gio");
    expect(formatSize(-5)).toBe("0 o");
  });

  it("la progression est dite en octets et la part reste entre 0 et 1", () => {
    expect(progressText(transfer())).toBe("150 Mio sur 600 Mio");
    expect(ratioOf(transfer())).toBe(0.25);
    expect(ratioOf(transfer({ sent: 900 * MIB }))).toBe(1);
    expect(ratioOf(transfer({ size: 0 }))).toBeNull();
  });
});

describe("les gestes qui ont un sens", () => {
  it("reprendre n'est offert que d'un envoi interrompu, jamais d'un envoi dont le serveur n'a plus rien", () => {
    expect(canResume(transfer({ status: "sending" }))).toBe(false);
    expect(canResume(transfer({ status: "interrupted" }))).toBe(true);
    expect(
      canResume(
        transfer({
          status: "interrupted",
          refusal: { kind: "no_space", missing: 5, remaining: 1 },
        }),
      ),
    ).toBe(true);
    expect(canResume(transfer({ status: "failed", refusal: { kind: "gone" } }))).toBe(false);
    expect(canResume(transfer({ status: "done" }))).toBe(false);
  });

  it("annuler est offert d'un envoi ouvert ; retirer de la liste d'un envoi fini seulement", () => {
    for (const status of ["sending", "interrupted"] as const) {
      expect(canCancel(transfer({ status }))).toBe(true);
      expect(canDismiss(transfer({ status }))).toBe(false);
      expect(isOpen(status)).toBe(true);
    }
    for (const status of ["done", "cancelled", "failed"] as const) {
      expect(canCancel(transfer({ status }))).toBe(false);
      expect(canDismiss(transfer({ status }))).toBe(true);
      expect(isOpen(status)).toBe(false);
    }
  });
});

describe("les phrases", () => {
  it("disent l'état, puis la raison quand il y en a une", () => {
    expect(statusText(transfer())).toBe("Envoi en cours.");
    expect(statusText(transfer({ status: "done", sent: 600 * MIB }))).toBe(
      "Le fichier est arrivé sur le serveur, vérifié.",
    );
    expect(statusText(transfer({ status: "cancelled" }))).toContain("partie reçue est supprimée");
    expect(
      statusText(
        transfer({
          status: "interrupted",
          refusal: { kind: "no_space", missing: 3 * GIB, remaining: 512 * MIB },
        }),
      ),
    ).toBe("Il manque 3 Gio sur le disque pour écrire ce fichier. Il resterait 512 Mio de libre.");
  });

  it("chaque refus a sa phrase, sans tiret cadratin et sans code brut", () => {
    const refusals = [
      { kind: "too_many" },
      { kind: "missing_right" },
      { kind: "rejected" },
      { kind: "source_unreadable" },
      { kind: "source_changed" },
      { kind: "gone" },
    ] as const;
    for (const refusal of refusals) {
      const text = refusalText(refusal);
      expect(text.length).toBeGreaterThan(10);
      expect(text).not.toContain("—");
      expect(text).not.toContain(refusal.kind);
    }
  });

  it("l'ouverture refusée est dite par son code, le temps d'attente en secondes entières", () => {
    expect(depositRefusalText({ kind: "wrong_password" })).toBe("Mot de passe incorrect.");
    expect(depositRefusalText({ kind: "too_many_attempts", waitS: 59.2 })).toContain("60 secondes");
    expect(depositRefusalText({ kind: "name_invalid" })).toContain("nom de ce fichier");
    expect(depositRefusalText({ kind: "no_space", missing: GIB, remaining: 0 })).toContain(
      "Il manque 1 Gio",
    );
    for (const kind of [
      "password_required",
      "busy",
      "too_many",
      "source_unreadable",
      "other",
    ] as const) {
      expect(depositRefusalText({ kind }).length).toBeGreaterThan(10);
    }
  });
});
