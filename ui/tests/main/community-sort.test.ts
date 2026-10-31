// @vitest-environment happy-dom
import { beforeEach, expect, it, vi } from 'vite-plus/test';

import { createCommunitySort } from '../../src/main/community-sort';

beforeEach(() => document.body.replaceChildren());

it('exposes two sort buttons and reverses the active metric without opening a menu', () => {
  const changed = vi.fn();
  const control = createCommunitySort(() => 'en', changed);
  document.body.append(control.element);
  expect(control.element.getAttribute('role')).toBe('group');
  expect(control.element.getAttribute('aria-label')).toBe('Sort');
  expect(control.element.querySelector('[role="menu"]')).toBeNull();
  const rows = [...control.element.querySelectorAll<HTMLButtonElement>('button')];
  expect(rows.map((row) => row.textContent)).toEqual(['Published', 'Weekly downloads']);
  expect(rows[0].getAttribute('aria-pressed')).toBe('true');
  rows[0].click();
  expect(control.value).toBe('published-asc');
  rows[1].click();
  expect(control.value).toBe('downloads-desc');
  rows[1].click();
  expect(control.value).toBe('downloads-asc');
  expect(rows[1].getAttribute('aria-pressed')).toBe('true');
  expect(rows[0].getAttribute('aria-pressed')).toBe('false');
  expect(rows[1].dataset.direction).toBe('asc');
  expect(changed).toHaveBeenCalledTimes(3);
});

it('preserves ordering through translation and disables both buttons during loading', () => {
  let locale: 'en' | 'zh-CN' = 'en';
  const control = createCommunitySort(
    () => locale,
    () => {},
  );
  document.body.append(control.element);
  const rows = [...control.element.querySelectorAll<HTMLButtonElement>('button')];
  rows[0].click();
  locale = 'zh-CN';
  control.render();
  expect(control.element.getAttribute('aria-label')).toBe('排序');
  expect(rows[0].textContent).toBe('发布时间');
  expect(rows[1].textContent).toBe('上周下载量');
  expect(control.value).toBe('published-asc');
  control.render(true);
  expect(rows.every((row) => row.disabled)).toBe(true);
  rows[1].click();
  expect(control.value).toBe('published-asc');
  control.render();
  expect(rows.every((row) => !row.disabled)).toBe(true);
});

it('keeps each directional SVG decorative while buttons expose names and direction help', () => {
  const control = createCommunitySort(
    () => 'en',
    () => {},
  );
  const rows = [...control.element.querySelectorAll<HTMLButtonElement>('button')];
  for (const row of rows) {
    expect(row.firstElementChild?.namespaceURI).toBe('http://www.w3.org/2000/svg');
    expect(row.firstElementChild?.getAttribute('aria-hidden')).toBe('true');
    expect(row.firstElementChild?.querySelectorAll('path[data-arrow]')).toHaveLength(2);
    expect(row.title).toBeTruthy();
  }
});

it('disables only download sorting while statistics are pending', () => {
  const changed = vi.fn();
  const control = createCommunitySort(() => 'en', changed);
  document.body.append(control.element);
  const published = control.element.querySelector<HTMLButtonElement>('[data-sort="published"]')!;
  const downloads = control.element.querySelector<HTMLButtonElement>('[data-sort="downloads"]')!;
  control.render(false, true);
  expect(published.disabled).toBe(false);
  expect(downloads.disabled).toBe(true);
  downloads.click();
  expect(changed).not.toHaveBeenCalled();
  expect(control.value).toBe('published-desc');
  published.click();
  expect(control.value).toBe('published-asc');
  control.render(false, true);
  expect(downloads.disabled).toBe(true);
  control.render();
  expect(downloads.disabled).toBe(false);
});
