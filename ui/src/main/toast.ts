import { CircleCheck, createElement } from 'lucide';

const TOAST_DURATION_MS = 4500;

/** Create the main window's polite, single-message notification region. */
export function createToastRegion(root: HTMLElement) {
  const region = document.createElement('div');
  region.className = 'toast-region';
  region.setAttribute('role', 'status');
  region.setAttribute('aria-live', 'polite');
  region.setAttribute('aria-atomic', 'true');
  root.append(region);

  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    show(message: string) {
      clearTimeout(timer);
      const toast = document.createElement('div');
      toast.className = 'toast';
      const label = document.createElement('span');
      label.textContent = message;
      toast.append(
        createElement(CircleCheck, { width: 18, height: 18, 'aria-hidden': 'true' }),
        label,
      );
      region.replaceChildren(toast);
      timer = setTimeout(() => region.replaceChildren(), TOAST_DURATION_MS);
    },
  };
}
