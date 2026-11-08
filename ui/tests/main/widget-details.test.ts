// @vitest-environment happy-dom
import { beforeEach, expect, it, vi } from 'vite-plus/test';

import type { LocalWidgetDetails } from '../../src/types/rust';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
import {
  createLocalWidgetDetails,
  renderWidgetDetailsContent,
} from '../../src/main/widget-details';

const details: LocalWidgetDetails = {
  config: {
    id: 'clock',
    title: 'Clock',
    minHostVersion: '0.1.1',
    logo: null,
    screenshots: [],
    platforms: ['macos', 'windows', 'linux'],
    size: { width: 360, height: 220 },
    dragMode: 'all',
    permissions: [],
    network: [],
  },
  compatibility: {
    state: 'compatible',
    currentVersion: '0.1.12',
    minHostVersion: '0.1.1',
    allowedByOverride: false,
  },
  description: 'A local clock',
  preview: 'data:image/png;base64,cG5n',
};
beforeEach(() => {
  document.body.replaceChildren();
  mocks.invoke.mockReset().mockResolvedValue(structuredClone(details));
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

it('renders empty permissions as plain text, and declared permissions and origins as lists', () => {
  const empty = renderWidgetDetailsContent(
    details.config,
    details.compatibility,
    null,
    [],
    'zh-CN',
  );
  expect(empty.querySelector('.community-permissions-empty')?.tagName).toBe('P');
  expect(empty.querySelector('.community-permissions-empty')?.textContent).toBe('无特殊权限');
  expect(empty.querySelector('.community-permissions')).toBeNull();
  expect(empty.querySelector('.community-detail-description')).toBeNull();
  const configured = structuredClone(details.config);
  configured.permissions = ['script', 'storage'];
  configured.network = ['https://api.example.com'];
  const content = renderWidgetDetailsContent(
    configured,
    details.compatibility,
    details.description,
    [],
    'en-US',
  );
  expect(
    [...content.querySelectorAll('.community-permissions li')].map((item) => item.textContent),
  ).toEqual(['JavaScript', 'Local storage']);
  expect(content.querySelector('.community-permissions-empty')).toBeNull();
  expect(content.querySelector('.community-network li')?.textContent).toBe(
    'https://api.example.com',
  );
  const networkTitle = [...content.querySelectorAll('h3')].find(
    (heading) => heading.textContent === 'Network access',
  )!;
  const hostTitle = [...content.querySelectorAll('h3')].find(
    (heading) => heading.textContent === 'Minimum Aidget version',
  )!;
  expect(networkTitle.compareDocumentPosition(hostTitle) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(
    Node.DOCUMENT_POSITION_FOLLOWING,
  );
});

it('shows the declared host version and sends incompatible widgets to update navigation', () => {
  const onUpdate = vi.fn();
  const compatibility = {
    ...details.compatibility,
    state: 'host_too_old' as const,
    minHostVersion: '0.2.0',
  };
  const content = renderWidgetDetailsContent(
    details.config,
    compatibility,
    null,
    [],
    'zh-CN',
    onUpdate,
  );
  expect(content.querySelector('.community-host-version')?.textContent).toContain('0.2.0');
  expect(content.querySelector('.community-host-incompatible')?.textContent).toBe(
    '不支持当前 aidget 版本，请先更新。',
  );
  content.querySelector<HTMLButtonElement>('.community-host-incompatible button')!.click();
  expect(onUpdate).toHaveBeenCalledOnce();
});

it('shows the local preview without community identity or header actions, translates and closes', async () => {
  let locale: 'en-US' | 'zh-CN' = 'en-US';
  const root = document.createElement('section');
  document.body.append(root);
  const controller = createLocalWidgetDetails(root, () => locale, vi.fn());
  await controller.show('clock');
  const dialog = root.querySelector('dialog')!;
  expect(dialog.open).toBe(true);
  expect(dialog.querySelector('h2')?.textContent).toBe('Clock');
  expect(dialog.querySelector<HTMLImageElement>('img')?.src).toBe(details.preview);
  expect(dialog.querySelector('.community-detail-identity')).toBeNull();
  expect(dialog.querySelector('.community-dialog-header button')).toBeNull();
  expect(dialog.textContent).toContain('A local clock');
  expect(document.activeElement).toBe(dialog);
  locale = 'zh-CN';
  controller.render();
  expect(dialog.querySelector('.community-permissions-empty')?.textContent).toBe('无特殊权限');
  root.querySelector<HTMLButtonElement>('.community-dialog-close')!.click();
  await vi.waitFor(() => expect(dialog.open).toBe(false));
  expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith('main_widget_details', { id: 'clock' });
});

it('closes local details before navigating to the update page', async () => {
  const incompatible = structuredClone(details);
  incompatible.compatibility = {
    state: 'host_too_old',
    currentVersion: '0.1.11',
    minHostVersion: '0.1.12',
    allowedByOverride: false,
  };
  mocks.invoke.mockResolvedValueOnce(incompatible);
  const onUpdate = vi.fn();
  const root = document.createElement('section');
  document.body.append(root);
  const controller = createLocalWidgetDetails(root, () => 'zh-CN', onUpdate);
  await controller.show('clock');
  const dialog = root.querySelector('dialog')!;
  dialog.querySelector<HTMLButtonElement>('.community-host-incompatible button')!.click();
  await vi.waitFor(() => expect(dialog.open).toBe(false));
  expect(onUpdate).toHaveBeenCalledOnce();
});

it('does not let an older or canceled read open a dialog, and exposes read failures', async () => {
  const pending = new Map<string, (value: LocalWidgetDetails) => void>();
  mocks.invoke.mockImplementation(
    (_command: string, args: { id: string }) =>
      new Promise<LocalWidgetDetails>((resolve) => pending.set(args.id, resolve)),
  );
  const root = document.createElement('section');
  document.body.append(root);
  const controller = createLocalWidgetDetails(root, () => 'en-US', vi.fn());
  const older = controller.show('old');
  const newer = controller.show('new');
  const latest = structuredClone(details);
  latest.config.title = 'Latest';
  pending.get('new')!(latest);
  await newer;
  pending.get('old')!(details);
  await older;
  expect(root.querySelector('h2')?.textContent).toBe('Latest');
  controller.close();
  const canceled = controller.show('canceled');
  controller.close();
  pending.get('canceled')!(details);
  await canceled;
  await vi.waitFor(() => expect(root.querySelector('dialog')!.open).toBe(false));
  mocks.invoke.mockRejectedValueOnce(new Error('missing widget'));
  await expect(controller.show('missing')).rejects.toThrow('missing widget');
  expect(root.querySelector('dialog')!.open).toBe(false);
});
