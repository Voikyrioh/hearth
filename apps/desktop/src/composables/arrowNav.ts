/**
 * Navigation aux flèches dans une liste de contrôles (barre des serveurs, navigation du
 * serveur) : haut/bas (ou gauche/droite) déplacent le focus, Début/Fin vont aux extrémités.
 * Tab continue de fonctionner normalement ; ceci s'ajoute, ne remplace rien.
 */
export function arrowNav(event: KeyboardEvent, container: HTMLElement | null, selector: string) {
  if (!container) return;
  const items = [...container.querySelectorAll<HTMLElement>(selector)];
  if (items.length === 0) return;
  const current = items.indexOf(document.activeElement as HTMLElement);
  let next: number;
  switch (event.key) {
    case "ArrowDown":
    case "ArrowRight":
      next = (current + 1) % items.length;
      break;
    case "ArrowUp":
    case "ArrowLeft":
      next = (current - 1 + items.length) % items.length;
      break;
    case "Home":
      next = 0;
      break;
    case "End":
      next = items.length - 1;
      break;
    default:
      return;
  }
  event.preventDefault();
  items[next]?.focus();
}
