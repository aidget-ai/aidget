const EXIT_ANIMATION = 'aidget-dialog-exit';
const EXIT_FALLBACK_MS = 240;

type ClosingState = {
  promise: Promise<void>;
  cancel: () => void;
};

const closingDialogs = new WeakMap<HTMLDialogElement, ClosingState>();

function prefersReducedMotion() {
  return (
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

function cancelClose(dialog: HTMLDialogElement) {
  closingDialogs.get(dialog)?.cancel();
}

/** Keep native dismissal, such as Escape, on the shared exit path. */
export function setupDialogAnimation(dialog: HTMLDialogElement) {
  dialog.addEventListener('cancel', (event) => {
    event.preventDefault();
    void closeDialog(dialog);
  });
  dialog.addEventListener('close', () => {
    dialog.removeAttribute('data-dialog-open');
    dialog.removeAttribute('data-dialog-closing');
  });
}

/** Open a modal dialog and start its enter animation. */
export function openDialog(dialog: HTMLDialogElement) {
  cancelClose(dialog);
  if (!dialog.open) {
    dialog.showModal();
  }
  dialog.setAttribute('data-dialog-open', '');
  dialog.focus();
}

/** Finish the exit animation before dismissing the native modal. */
export function closeDialog(dialog: HTMLDialogElement) {
  if (!dialog.open) {
    return Promise.resolve();
  }
  const pending = closingDialogs.get(dialog);
  if (pending) {
    return pending.promise;
  }

  dialog.removeAttribute('data-dialog-open');
  if (prefersReducedMotion()) {
    dialog.close();
    return Promise.resolve();
  }

  let settled = false;
  let timer: number | undefined;
  let resolveClose!: () => void;
  const promise = new Promise<void>((resolve) => {
    resolveClose = resolve;
  });

  function finish(close: boolean) {
    if (settled) {
      return;
    }
    settled = true;
    window.clearTimeout(timer);
    dialog.removeEventListener('animationend', onAnimationEnd);
    closingDialogs.delete(dialog);
    dialog.removeAttribute('data-dialog-closing');
    if (close && dialog.open) {
      dialog.close();
    }
    resolveClose();
  }

  function onAnimationEnd(event: AnimationEvent) {
    if (event.target === dialog && event.animationName === EXIT_ANIMATION) {
      finish(true);
    }
  }

  closingDialogs.set(dialog, {
    promise,
    cancel: () => finish(false),
  });
  dialog.setAttribute('data-dialog-closing', '');
  dialog.addEventListener('animationend', onAnimationEnd);
  timer = window.setTimeout(() => finish(true), EXIT_FALLBACK_MS);
  return promise;
}
