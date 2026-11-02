import type { Locale } from './locale';
import type { Theme } from './preferences';

export interface WidgetSummary {
  id: string;
  title: string;
  packageName: string | null;
  revision: number;
  token: string;
  width: number;
  height: number;
  enabled: boolean;
  createdAt: number;
}

export interface DeletedWidgetSummary {
  trashName: string;
  id: string;
  title: string;
  width: number;
  height: number;
  hasPreview: boolean;
  createdAt: number;
}

export interface MainState {
  widgets: WidgetSummary[];
  deletedWidgets: DeletedWidgetSummary[];
  broken: { id: string; error: string }[];
  version: string;
  theme: Theme;
  language: Locale;
  trayClickAction: TrayClickAction;
  lastPage: MainPage;
  githubToken: GithubTokenSummary | null;
  autoDownload: boolean;
}

export interface GithubTokenSummary {
  masked: string;
}

export type MainPage = 'local-widgets' | 'community-widgets' | 'integrations' | 'general' | 'about';
export type WidgetFilter = 'all' | 'enabled' | 'disabled' | 'deleted';
export type WidgetSortOrder = 'newest' | 'oldest';
export type TrayClickAction = 'menu' | 'window';

export function sortedActiveWidgets(
  widgets: WidgetSummary[],
  order: WidgetSortOrder,
): WidgetSummary[] {
  return [...widgets].sort((left, right) => {
    const delta = left.createdAt - right.createdAt;
    if (delta !== 0) {
      return order === 'oldest' ? delta : -delta;
    }
    return left.id.localeCompare(right.id);
  });
}

export function matchesWidgetFilter(widget: WidgetSummary, filter: WidgetFilter): boolean {
  if (filter === 'deleted') {
    return false;
  }
  return filter === 'all' || (filter === 'enabled' ? widget.enabled : !widget.enabled);
}

export function filteredWidgets(widgets: WidgetSummary[], filter: WidgetFilter): WidgetSummary[] {
  return widgets.filter((widget) => matchesWidgetFilter(widget, filter));
}

export function sortedDeletedWidgets(
  widgets: DeletedWidgetSummary[],
  order: WidgetSortOrder,
): DeletedWidgetSummary[] {
  return [...widgets].sort((left, right) => {
    const delta = left.createdAt - right.createdAt;
    if (delta !== 0) {
      return order === 'oldest' ? delta : -delta;
    }
    return left.trashName.localeCompare(right.trashName);
  });
}

export function previewKey(widget: WidgetSummary): string {
  return `${widget.revision}-${widget.token}`;
}

export function previewNeedsRefresh(
  previous: WidgetSummary | undefined,
  next: WidgetSummary,
): boolean {
  return (
    next.enabled && (!previous || previewKey(previous) !== previewKey(next) || !previous.enabled)
  );
}

export function isEmpty(state: MainState): boolean {
  return state.widgets.length === 0;
}
