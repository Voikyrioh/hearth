import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, ref } from "vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import { LinkCommandError } from "@/link";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useToastsStore } from "@/stores/toasts";
import { mountContext } from "@/test/mount";
import { useAccountActions } from "./useAccountActions";
import { useServerAction } from "./useServerAction";

// HRT-78 (BR-GRANT-010, 011) : un refus du serveur faute du droit de gérer les services ferme la fenêtre ouverte
// avec « Tu n'as plus le droit de gérer les services de ce serveur. », relit le droit, et les boutons se
// désactivent. Aucune fenêtre d'action de service n'existe encore : c'est le mécanisme commun qui est éprouvé
// DE BOUT EN BOUT (C2 de la review) : un vrai composable d'actions (`useAccountActions`) reçoit le refus du pont, en
// tire le rapport `right_lost` avec l'identifiant du serveur, et la VRAIE fenêtre `AdminActDialog` se ferme dessus.
beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  document.body.innerHTML = "";
});

const MISSING = new LinkCommandError({ kind: "missing_services_right" });

function probe(serverId: string | null) {
  let action: ReturnType<typeof useServerAction> | undefined;
  const Probe = defineComponent({
    setup() {
      action = useServerAction(() => serverId);
      return () => h("div");
    },
  });
  return {
    mount: (global: object) => mount(Probe, { global }),
    run: () => {
      if (!action) throw new Error("pas monté");
      return action.run(() => Promise.reject(MISSING));
    },
    state: () => action,
  };
}

describe("useServerAction : le refus faute du droit", () => {
  it("dit « Tu n'as plus le droit… », relit le droit du serveur et ne parle jamais de rôle", async () => {
    const ctx = await mountContext();
    await useServiceRightsStore().load("forge");
    ctx.bridge.setServiceRightOnServer("forge", "marie", false);
    const p = probe("forge");
    p.mount(ctx.global);
    expect(await p.run()).toBeNull();
    await flushPromises();
    const messages = useToastsStore().items.map((toast) => toast.message);
    expect(messages).toEqual(["Tu n'as plus le droit de gérer les services de ce serveur."]);
    expect(messages.join(" ")).not.toMatch(/administrateur|rôle/i);
    expect(p.state()?.rightLost.value).toBe(true);
    expect(p.state()?.failedReport()).toEqual({ kind: "right_lost" });
    // Le droit a été relu : il est maintenant retiré.
    expect(useServiceRightsStore().of("forge")?.rights?.granted).toBe(false);
  });

  it("une autre action réussie remet le marqueur à zéro", async () => {
    const ctx = await mountContext();
    const p = probe("forge");
    p.mount(ctx.global);
    await p.run();
    expect(p.state()?.rightLost.value).toBe(true);
    await p.state()?.run(async () => ({ kind: "done" }));
    expect(p.state()?.rightLost.value).toBe(false);
    expect(p.state()?.failedReport()).toEqual({ kind: "failed" });
  });
});

describe("la fenêtre ouverte se ferme quand le serveur refuse faute du droit", () => {
  async function openDialog() {
    const ctx = await mountContext();
    await useServiceRightsStore().load("forge");
    expect(useServiceRightsStore().of("forge")?.rights?.granted).toBe(true);
    const accounts = (await ctx.bridge.listAccounts("forge")).accounts;
    const target = accounts.find((account) => account.username === "paul");
    if (!target) throw new Error("compte d'essai absent");
    // Le serveur retire le droit, puis refuse l'acte pour ce motif (la vraie forme du refus, `details.missing`).
    ctx.bridge.setServiceRightOnServer("forge", "marie", false);
    vi.spyOn(ctx.bridge, "changeAccountRole").mockRejectedValue(MISSING);
    const open = ref(true);
    const Host = defineComponent({
      setup() {
        const actions = useAccountActions(() => "forge");
        return () =>
          h(AdminActDialog, {
            open: open.value,
            serverId: "forge",
            kind: "account_role",
            title: "Acte",
            submitLabel: "Valider",
            perform: (password: string | null) => actions.changeRole(target, "admin", password),
            onClose: () => {
              open.value = false;
            },
          });
      },
    });
    const wrapper = mount(Host, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    return { ctx, wrapper, open };
  }

  async function submit() {
    const input = document.querySelector<HTMLInputElement>("input[type=password]");
    if (input) {
      input.value = "Correct-Horse-9";
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
    await flushPromises();
    document.querySelector<HTMLFormElement>("dialog form")?.dispatchEvent(new Event("submit"));
    await flushPromises();
  }

  it("fenêtre fermée, texte prévu, droit relu : les boutons de service se désactivent", async () => {
    const { ctx, wrapper, open } = await openDialog();
    const reads = ctx.bridge.rightsReads.length;
    await submit();
    expect(open.value).toBe(false);
    expect(useToastsStore().items.map((toast) => toast.message)).toEqual([
      "Tu n'as plus le droit de gérer les services de ce serveur.",
    ]);
    expect(ctx.bridge.rightsReads.length).toBe(reads + 1);
    expect(useServiceRightsStore().of("forge")?.rights?.granted).toBe(false);
    wrapper.unmount();
  });
});
