import type { App } from "vue";
import { reportUiError } from "./report";

/**
 * Filet de sécurité global : toute erreur non rattrapée par une frontière d'erreur
 * (rendu, gestionnaire d'événement, rejet de promesse, erreur de script) est notifiée
 * discrètement et journalisée. L'interface ne devient jamais un écran blanc.
 */
export function installErrorHandlers(app: App, target: Window = window): void {
  app.config.errorHandler = (error, _instance, info) => reportUiError(error, `vue:${info}`);
  target.addEventListener("unhandledrejection", (event) =>
    reportUiError(event.reason, "unhandledrejection"),
  );
  target.addEventListener("error", (event) =>
    reportUiError(event.error ?? event.message, "window"),
  );
}
