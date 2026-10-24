import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import type { ServiceOperation } from "@/link";
import { hasTooltip, tooltipFor } from "@/test/tooltip";
import DeployProgressDialog from "./DeployProgressDialog.vue";

// L'infobulle du bouton « Annuler le déploiement » dit la bonne chose à chaque moment (HRT-69, BR-DEPLOY-016) :
// rien avant la première annonce de l'opération, « L'annulation est en cours » pendant l'annulation, « Il est trop tard » après la
// création.
beforeEach(() => setActivePinia(createPinia()));

const op = (patch: Partial<ServiceOperation> = {}): ServiceOperation => ({
  id: "01OP",
  key: "h_web",
  kind: "deploy",
  status: "running",
  step: "pull",
  steps: [],
  percent: null,
  itemsDone: null,
  bytesDone: null,
  bytesTotal: null,
  etaS: null,
  cancellable: true,
  decision: null,
  cause: null,
  ...patch,
});

function dialog(operation: ServiceOperation | null) {
  mount(DeployProgressDialog, {
    attachTo: document.body,
    props: {
      open: true,
      serverId: "forge",
      name: "web",
      operation,
      ended: null,
      unknown: false,
      service: null,
      performCancel: async () => ({ kind: "done" as const }),
    },
  });
  return document.querySelector<HTMLElement>("[data-deploy-cancel]");
}

describe("le bouton « Annuler le déploiement »", () => {
  it("ne dit rien avant la première annonce de l'opération", () => {
    const button = dialog(null);
    expect(button?.getAttribute("aria-disabled")).toBe("true");
    expect(hasTooltip()).toBe(false);
  });

  it("est actif pendant la récupération", () => {
    expect(dialog(op())?.getAttribute("aria-disabled")).toBeNull();
  });

  it("dit « L'annulation est en cours » pendant l'annulation, pas « Il est trop tard »", () => {
    const button = dialog(op({ status: "cancelling", cancellable: false }));
    expect(button?.getAttribute("aria-disabled")).toBe("true");
    expect(
      tooltipFor({ attributes: (name) => button?.getAttribute(name) ?? undefined }).text(),
    ).toBe("L'annulation est en cours…");
    expect(document.querySelector("[data-deploy-too-late]")).toBeNull();
  });

  it("dit « Il est trop tard pour annuler » une fois la création commencée", () => {
    const button = dialog(op({ step: "create", cancellable: false }));
    expect(
      tooltipFor({ attributes: (name) => button?.getAttribute(name) ?? undefined }).text(),
    ).toBe("Il est trop tard pour annuler : le service est en cours de création.");
    expect(document.querySelector("[data-deploy-too-late]")).not.toBeNull();
  });
});
