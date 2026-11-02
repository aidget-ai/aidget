// @vitest-environment happy-dom
import { beforeEach, expect, it, vi } from 'vite-plus/test';

import { createCommunityCard } from '../../src/main/community-card';
import { createCommunityMedia } from '../../src/main/community-media';
import { communityEntry } from '../helpers/community';
import { mockIntersections } from '../helpers/intersection';
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }));
beforeEach(() => {
  vi.restoreAllMocks();
  document.body.replaceChildren();
  mockIntersections();
  mocks.invoke.mockReset().mockResolvedValue('data:image/png;base64,test');
  vi.spyOn(HTMLImageElement.prototype, 'src', 'set').mockImplementation(function (
    this: HTMLImageElement,
  ) {
    this.dispatchEvent(new Event('load'));
  });
});
it('keeps logo and screenshot nodes when paths or metadata change but digests remain identical', async () => {
  const root = document.createElement('section');
  document.body.append(root);
  const entry = communityEntry(1);
  entry.entry.config.logo = 'logo.png';
  entry.entry.files.push({ path: 'logo.png', size: 32, sha256: 'f'.repeat(64) });
  const details = vi.fn();
  const media = createCommunityMedia(() => 'en');
  const card = createCommunityCard(entry, 'a'.repeat(40), media, root, () => 'en', details);
  root.append(card.element);
  card.update(entry, 'a'.repeat(40), false, 'Install');
  await vi.waitFor(() => expect(root.querySelectorAll('img')).toHaveLength(2));
  const logo = root.querySelector('.community-logo img');
  const screenshot = root.querySelector('.preview-image');
  const next = structuredClone(entry);
  next.entry.config.logo = 'icon.png';
  next.entry.files[1].path = 'icon.png';
  next.entry.config.title = 'Updated title';
  next.entry.package.version = '1.1.0';
  card.update(next, 'b'.repeat(40), false, 'Install');
  expect(root.querySelector('.community-logo img')).toBe(logo);
  expect(root.querySelector('.preview-image')).toBe(screenshot);
  expect(root.textContent).toContain('Updated title');
  expect(root.textContent).toContain('v1.1.0');
  root.querySelector<HTMLButtonElement>('.community-name')!.click();
  expect(details).toHaveBeenCalledWith(next, 'b'.repeat(40));
  expect(mocks.invoke).toHaveBeenCalledTimes(2);
  expect(root.querySelector('button button')).toBeNull();
});
it('hands the displayed revision to details after an update', () => {
  const root = document.createElement('section');
  document.body.append(root);
  const entry = communityEntry(1);
  const details = vi.fn();
  const media = createCommunityMedia(() => 'en');
  const card = createCommunityCard(entry, 'a'.repeat(40), media, root, () => 'en', details);
  root.append(card.element);
  const next = structuredClone(entry);
  next.entry.config.title = 'Updated title';
  card.update(next, 'b'.repeat(40), false, 'Install');

  root.querySelector<HTMLButtonElement>('.community-name')!.click();

  expect(details).toHaveBeenCalledWith(next, 'b'.repeat(40));
});
