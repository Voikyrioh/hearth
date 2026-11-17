import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import AdminActPanel from "@/components/organisms/AdminActPanel.vue";
import { useAdminAct } from "@/composables/useAdminAct";
import type { ActReport } from "@/composables/useReauth";
import type { AdminActKind } from "@/link";
import { startedApp } from "@/test/app";
import { OWN, typeInto } from "@/test/confirm";
import AdminActDialog from "./AdminActDialog.vue";

// HRT-245 : la confirmation par mot de passe posée dans une PAGE (le panneau, sans fenêtre) donne le même résultat que la
// fenêtre, état par état : agent sans confirmation, poste sans clé, acte de pilotage en état normal, élévation couvrante,
// champ demandé, refus, attente, état changé entre l'ouverture et l'envoi. Les mêmes scénarios sont joués sur les deux
// enveloppes ; le mot de passe n'est jamais gardé hors du champ et il est vidé après chaque envoi.

/** Le champ « Votre mot de passe », dans la fenêtre comme dans la page. */
const reauthField = () => document.querySelector<HTMLInputElement>("[data-reauth-field] input");

const PAUL = "SIMACCOUNT0002";

/** Un acte qui passe par le pont simulé : il applique les règles de l'agent (mot de passe, élévation, état de confiance). */
function viaBridge(ctx: Awaited<ReturnType<typeof startedApp>>, sent: Array<string | null>) {
  return async (adminPassword: string | null): Promise<ActReport> => {
    sent.push(adminPassword);
    const outcome = await ctx.bridge.closeAccountSessions("forge", PAUL, adminPassword);
    return outcome.kind === "done" ? { kind: "done" } : (outcome as ActReport);
  };
}

afterEach(() => {
  document.body.innerHTML = "";
  vi.useRealTimers();
});

type Mode = "dialog" | "panel";
const MODES: Mode[] = ["dialog", "panel"];

/** Une page minimale : le panneau, son bouton, rien d'une fenêtre. */
function pageHost(
  kind: AdminActKind,
  perform: (adminPassword: string | null) => Promise<ActReport>,
  finished: () => void,
) {
  return defineComponent({
    setup() {
      const act = useAdminAct({
        serverId: () => "forge",
        kind: () => kind,
        role: () => null,
        active: () => true,
        canSubmit: () => true,
        blockedReason: () => undefined,
        perform,
        onFinished: finished,
      });
      return () =>
        h("form", { onSubmit: (event: Event) => event.preventDefault() }, [
          h("fieldset", [
            h(
              AdminActPanel,
              { act, serverId: "forge" },
              { default: () => h("input", { placeholder: "saisie" }) },
            ),
          ]),
          act.error.value ? h("p", { "data-page-error": "" }, act.error.value) : null,
          h(
            "button",
            {
              type: "button",
              "data-page-launch": "",
              "aria-disabled": act.canSend.value ? undefined : "true",
              onClick: () => {
                if (act.canSend.value) void act.submit();
              },
            },
            "Lancer",
          ),
        ]);
    },
  });
}

async function mountMode(
  mode: Mode,
  ctx: Awaited<ReturnType<typeof startedApp>>,
  kind: AdminActKind,
  perform: (adminPassword: string | null) => Promise<ActReport>,
) {
  const finished = vi.fn();
  const wrapper =
    mode === "dialog"
      ? mount(AdminActDialog, {
          props: {
            open: true,
            serverId: "forge",
            kind,
            title: "Lancer ?",
            submitLabel: "Lancer",
            perform,
            onClose: finished,
          },
          slots: { default: () => h("input", { placeholder: "saisie" }) },
          global: { plugins: [ctx.pinia] },
          attachTo: document.body,
        })
      : mount(pageHost(kind, perform, finished), {
          global: { plugins: [ctx.pinia] },
          attachTo: document.body,
        });
  await flushPromises();
  return { wrapper, finished };
}

async function launch(mode: Mode) {
  const button =
    mode === "dialog"
      ? [...document.querySelectorAll<HTMLButtonElement>("dialog button")].find(
          (b) => b.textContent?.trim() === "Lancer",
        )
      : document.querySelector<HTMLButtonElement>("[data-page-launch]");
  button?.click();
  await flushPromises();
}

describe.each(MODES)("la confirmation par mot de passe, enveloppe « %s » (HRT-245)", (mode) => {
  it("demande « Votre mot de passe » quand l'agent l'annonce et n'envoie rien sans lui", async () => {
    const ctx = await startedApp();
    const sent: Array<string | null> = [];
    const { wrapper } = await mountMode(mode, ctx, "sessions_revoke", async (password) => {
      sent.push(password);
      return { kind: "done" };
    });
    expect(reauthField()).not.toBeNull();
    expect(document.body.textContent).toContain("Votre mot de passe");
    await launch(mode);
    expect(sent).toEqual([]);
    wrapper.unmount();
  });

  it("envoie le mot de passe une fois, vide le champ et rend la main", async () => {
    const ctx = await startedApp();
    const sent: Array<string | null> = [];
    const { wrapper, finished } = await mountMode(
      mode,
      ctx,
      "sessions_revoke",
      viaBridge(ctx, sent),
    );
    typeInto(reauthField(), OWN);
    await flushPromises();
    await launch(mode);
    expect(sent).toEqual([OWN]);
    expect(finished).toHaveBeenCalledTimes(1);
    expect(reauthField()?.value ?? "").toBe("");
    expect(JSON.stringify(ctx.pinia.state.value)).not.toContain(OWN);
    wrapper.unmount();
  });

  it("dit un mot de passe faux sous le champ, vide le champ et ne rend pas la main", async () => {
    const ctx = await startedApp();
    const { wrapper, finished } = await mountMode(mode, ctx, "sessions_revoke", viaBridge(ctx, []));
    typeInto(reauthField(), "Faux-Mot-De-Passe-1");
    await flushPromises();
    await launch(mode);
    expect(document.body.textContent).toContain("Mot de passe incorrect.");
    expect(reauthField()?.value).toBe("");
    expect(finished).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("dit l'attente de l'agent sous le champ", async () => {
    const ctx = await startedApp();
    const { wrapper } = await mountMode(mode, ctx, "sessions_revoke", async () => ({
      kind: "refused",
      refusal: { kind: "too_many_attempts", retry_after_s: 42 },
    }));
    typeInto(reauthField(), "Faux-Mot-De-Passe-1");
    await flushPromises();
    await launch(mode);
    expect(document.body.textContent).toContain("42");
    wrapper.unmount();
  });

  it("ne demande rien pour un acte de pilotage en état normal, et le mot de passe en alerte", async () => {
    for (const [trust, asks] of [
      ["normal", false],
      ["alert", true],
    ] as const) {
      const ctx = await startedApp();
      ctx.bridge.reauth.setTrust("forge", trust);
      const sent: Array<string | null> = [];
      const { wrapper } = await mountMode(mode, ctx, "service_stop", viaBridge(ctx, sent));
      expect(reauthField() !== null, trust).toBe(asks);
      if (asks) {
        typeInto(reauthField(), OWN);
        await flushPromises();
      }
      await launch(mode);
      expect(sent, trust).toEqual([asks ? OWN : null]);
      wrapper.unmount();
      document.body.innerHTML = "";
    }
  });

  it("dit de mettre l'agent à jour quand il n'annonce pas la confirmation, sans rien envoyer", async () => {
    const ctx = await startedApp();
    ctx.bridge.reauth.setSupported("forge", false);
    const perform = vi.fn(async (): Promise<ActReport> => ({ kind: "done" }));
    const { wrapper } = await mountMode(mode, ctx, "sessions_revoke", perform);
    expect(reauthField()).toBeNull();
    expect(document.querySelector("[data-reauth-agent-old]")).not.toBeNull();
    expect(document.querySelector("[data-reauth-settings]")).not.toBeNull();
    await launch(mode);
    expect(perform).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("explique un poste sans clé et ne laisse rien partir", async () => {
    const ctx = await startedApp();
    ctx.bridge.security.setDevice("forge", "none", false);
    const perform = vi.fn(async (): Promise<ActReport> => ({ kind: "done" }));
    const { wrapper } = await mountMode(mode, ctx, "sessions_revoke", perform);
    expect(document.querySelector("[data-reauth-no-key]")).not.toBeNull();
    expect(reauthField()).toBeNull();
    expect(document.querySelector("[data-reauth-reconnect]")).not.toBeNull();
    await launch(mode);
    expect(perform).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("n'a pas de champ pendant l'élévation d'un acte couvert, et dit le temps restant", async () => {
    const ctx = await startedApp();
    const first = await mountMode(mode, ctx, "sessions_revoke", viaBridge(ctx, []));
    typeInto(reauthField(), OWN);
    await flushPromises();
    await launch(mode);
    first.wrapper.unmount();
    document.body.innerHTML = "";
    const { wrapper } = await mountMode(mode, ctx, "sessions_revoke", async () => ({
      kind: "done",
    }));
    expect(reauthField()).toBeNull();
    expect(document.querySelector("[data-reauth-elevated]")?.textContent).toMatch(/\d+ min \d+ s/);
    wrapper.unmount();
  });

  it("redemande le mot de passe, sans perdre la saisie de l'acte, quand l'état passe de normal à alerte avant l'envoi", async () => {
    const ctx = await startedApp();
    ctx.bridge.reauth.setTrust("forge", "normal");
    const sent: Array<string | null> = [];
    const { wrapper, finished } = await mountMode(mode, ctx, "service_stop", viaBridge(ctx, sent));
    typeInto(document.querySelector<HTMLInputElement>("input[placeholder]"), "ma saisie");
    ctx.bridge.reauth.setTrust("forge", "alert");
    await launch(mode);
    expect(finished).not.toHaveBeenCalled();
    expect(reauthField()).not.toBeNull();
    expect(document.body.textContent).toMatch(/mot de passe/i);
    typeInto(reauthField(), OWN);
    await flushPromises();
    await launch(mode);
    expect(sent).toEqual([null, OWN]);
    expect(finished).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });
});
