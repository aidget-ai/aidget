// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vite-plus/test';

import { closeDialog, openDialog, setupDialogAnimation } from '../../src/utils/dialog';

beforeEach(() => {
  vi.useFakeTimers();
  document.body.replaceChildren();
  vi.spyOn(HTMLDialogElement.prototype, 'showModal').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = true;
  });
  vi.spyOn(HTMLDialogElement.prototype, 'close').mockImplementation(function (
    this: HTMLDialogElement,
  ) {
    this.open = false;
    this.dispatchEvent(new Event('close'));
  });
});

afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

function createDialog() {
  const dialog = document.createElement('dialog');
  dialog.tabIndex = -1;
  setupDialogAnimation(dialog);
  document.body.append(dialog);
  return dialog;
}

function finishExit(dialog: HTMLDialogElement) {
  const event = new Event('animationend');
  Object.defineProperty(event, 'animationName', { value: 'aidget-dialog-exit' });
  dialog.dispatchEvent(event);
}

it('opens a modal dialog and marks it for the enter animation', () => {
  const dialog = createDialog();

  openDialog(dialog);

  expect(dialog.open).toBe(true);
  expect(dialog.hasAttribute('data-dialog-open')).toBe(true);
  expect(document.activeElement).toBe(dialog);
});

it('keeps the dialog open until its exit animation finishes', async () => {
  const dialog = createDialog();
  openDialog(dialog);

  const closing = closeDialog(dialog);

  expect(dialog.open).toBe(true);
  expect(dialog.hasAttribute('data-dialog-open')).toBe(false);
  expect(dialog.hasAttribute('data-dialog-closing')).toBe(true);

  finishExit(dialog);
  await closing;

  expect(dialog.open).toBe(false);
  expect(dialog.hasAttribute('data-dialog-closing')).toBe(false);
});

it('uses the exit fallback when the animation end event is unavailable', async () => {
  const dialog = createDialog();
  openDialog(dialog);

  const closing = closeDialog(dialog);
  await vi.advanceTimersByTimeAsync(240);
  await closing;

  expect(dialog.open).toBe(false);
});

it('intercepts Escape and closes through the same exit path', async () => {
  const dialog = createDialog();
  openDialog(dialog);

  const cancel = new Event('cancel', { cancelable: true });
  dialog.dispatchEvent(cancel);

  expect(cancel.defaultPrevented).toBe(true);
  expect(dialog.open).toBe(true);
  expect(dialog.hasAttribute('data-dialog-closing')).toBe(true);

  await vi.advanceTimersByTimeAsync(240);
  expect(dialog.open).toBe(false);
});

it('shares an in-flight close request', async () => {
  const dialog = createDialog();
  openDialog(dialog);

  const first = closeDialog(dialog);
  const second = closeDialog(dialog);

  expect(second).toBe(first);
  finishExit(dialog);
  await first;
  expect(dialog.open).toBe(false);
});

it('reopens an exiting dialog without dismissing it', async () => {
  const dialog = createDialog();
  openDialog(dialog);

  const closing = closeDialog(dialog);
  openDialog(dialog);
  await closing;

  expect(dialog.open).toBe(true);
  expect(dialog.hasAttribute('data-dialog-open')).toBe(true);
  expect(dialog.hasAttribute('data-dialog-closing')).toBe(false);
});

it('closes immediately when reduced motion is requested', () => {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({ matches: true })),
  );
  const dialog = createDialog();
  openDialog(dialog);

  const closing = closeDialog(dialog);

  expect(dialog.open).toBe(false);
  return closing;
});
