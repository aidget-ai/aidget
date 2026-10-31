import { describe, expect, it } from 'vite-plus/test';

import {
  filteredWidgets,
  isEmpty,
  matchesWidgetFilter,
  previewKey,
  previewNeedsRefresh,
  sortedActiveWidgets,
  sortedDeletedWidgets,
  type MainState,
  type WidgetSummary,
} from '../../src/main/model';

const base: WidgetSummary = {
  id: 'todo',
  title: 'Todo',
  packageName: null,
  revision: 2,
  token: 'abc',
  width: 360,
  height: 220,
  enabled: true,
  createdAt: 10,
};

describe('main window widgets', () => {
  it('sorts all widgets by creation time in both directions', () => {
    const widgets = [
      { ...base, id: 'later', createdAt: 200 },
      { ...base, id: 'earlier', createdAt: 100 },
    ];
    expect(sortedActiveWidgets(widgets, 'newest').map((widget) => widget.id)).toEqual([
      'later',
      'earlier',
    ]);
    expect(sortedActiveWidgets(widgets, 'oldest').map((widget) => widget.id)).toEqual([
      'earlier',
      'later',
    ]);
  });

  it('filters widgets by their enabled state', () => {
    const widgets = [
      { ...base, id: 'alpha' },
      { ...base, id: 'beta', enabled: false },
    ];

    expect(filteredWidgets(widgets, 'all').map((widget) => widget.id)).toEqual(['alpha', 'beta']);
    expect(filteredWidgets(widgets, 'enabled').map((widget) => widget.id)).toEqual(['alpha']);
    expect(filteredWidgets(widgets, 'disabled').map((widget) => widget.id)).toEqual(['beta']);
    expect(matchesWidgetFilter(base, 'enabled')).toBe(true);
    expect(matchesWidgetFilter(base, 'disabled')).toBe(false);
    expect(matchesWidgetFilter(base, 'deleted')).toBe(false);
  });

  it('refreshes static previews when enabled content changes', () => {
    expect(previewKey(base)).toBe('2-abc');
    expect(previewNeedsRefresh(undefined, base)).toBe(true);
    expect(previewNeedsRefresh(base, { ...base })).toBe(false);
    expect(previewNeedsRefresh(base, { ...base, revision: 3 })).toBe(true);
    expect(previewNeedsRefresh(base, { ...base, token: 'next' })).toBe(true);
    expect(previewNeedsRefresh(base, { ...base, enabled: false })).toBe(false);
    expect(previewNeedsRefresh({ ...base, enabled: false }, base)).toBe(true);
  });

  it('recognizes an empty widget collection', () => {
    const state: MainState = {
      widgets: [],
      deletedWidgets: [],
      broken: [],
      version: '0.1.0',
      theme: 'system',
      language: 'system',
      trayClickAction: 'menu',
      lastPage: 'integrations',
      githubToken: null,
      autoDownload: true,
    };
    expect(isEmpty(state)).toBe(true);
    expect(isEmpty({ ...state, widgets: [base] })).toBe(false);
  });

  it('keeps deleted widgets out of the ordinary filters', () => {
    const deleted = {
      trashName: 'todo-20261022T000000',
      id: 'todo',
      title: 'Todo',
      packageName: null,
      width: 360,
      height: 220,
      hasPreview: true,
      createdAt: 1_761_158_400_000,
    };
    const state: MainState = {
      widgets: [base],
      deletedWidgets: [deleted],
      broken: [],
      version: '0.1.0',
      theme: 'system',
      language: 'system',
      trayClickAction: 'menu',
      lastPage: 'integrations',
      githubToken: null,
      autoDownload: true,
    };

    expect(state.deletedWidgets[0].hasPreview).toBe(true);
    expect(filteredWidgets(state.widgets, 'deleted')).toEqual([]);
  });

  it('sorts trashed widgets by creation time in both directions', () => {
    const widgets = [
      {
        trashName: 'later',
        id: 'later',
        title: 'Later',
        width: 360,
        height: 220,
        hasPreview: false,
        createdAt: 200,
      },
      {
        trashName: 'earlier',
        id: 'earlier',
        title: 'Earlier',
        width: 360,
        height: 220,
        hasPreview: true,
        createdAt: 100,
      },
    ];

    expect(sortedDeletedWidgets(widgets, 'newest').map((widget) => widget.id)).toEqual([
      'later',
      'earlier',
    ]);
    expect(sortedDeletedWidgets(widgets, 'oldest').map((widget) => widget.id)).toEqual([
      'earlier',
      'later',
    ]);
  });
});
