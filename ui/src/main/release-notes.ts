import DOMPurify from 'dompurify';
import { marked, type Token } from 'marked';

import type { ReleaseNote, UpdateNotes } from './update';

const CHINESE_MARKER = '<!-- lang:zh -->';
const ALLOWED_TAGS = [
  'a',
  'blockquote',
  'code',
  'em',
  'h3',
  'h4',
  'h5',
  'h6',
  'li',
  'ol',
  'p',
  'pre',
  'strong',
  'ul',
];

/** Select one language's changelog, excluding install and thanks footers. */
export function localizedChangelog(body: string, language: 'en' | 'zh') {
  const separator = body.indexOf(CHINESE_MARKER);
  if (separator < 0 && language === 'zh') {
    throw new Error('release notes are missing the Chinese-language separator');
  }
  const selected =
    separator < 0
      ? body
      : language === 'zh'
        ? body.slice(separator + CHINESE_MARKER.length)
        : body.slice(0, separator);
  const cutoff = language === 'zh' ? '### 安装说明' : '### Installation';
  const lines = selected.split(/\r?\n/);
  const end = lines.findIndex((line) => line.trim() === cutoff);
  const changelog = (end < 0 ? lines : lines.slice(0, end)).join('\n').trim();
  if (!changelog) {
    throw new Error(`release notes are missing the ${language} changelog`);
  }
  return changelog;
}

interface CategoryEntry {
  version: string;
  markdown: string;
}

interface Category {
  title: string;
  entries: CategoryEntry[];
}

/** Merge the same changelog headings across releases while preserving provenance. */
export function renderReleaseNotes(
  root: HTMLElement,
  notes: UpdateNotes,
  language: 'en' | 'zh',
  otherLabel: string,
  openLink: (url: string) => void,
) {
  const categories = new Map<string, Category>();
  for (const release of notes.releases) {
    for (const category of categoriesFor(release, language, otherLabel)) {
      const merged = categories.get(category.title) ?? { title: category.title, entries: [] };
      merged.entries.push(...category.entries);
      categories.set(category.title, merged);
    }
  }

  const purifier = DOMPurify(window);
  const clean = (html: string) => {
    const result = purifier.sanitize(html, {
      ALLOWED_TAGS,
      ALLOWED_ATTR: ['href', 'title'],
      ALLOW_DATA_ATTR: false,
      ALLOW_UNKNOWN_PROTOCOLS: true,
    });
    return result;
  };
  const showVersion = notes.releases.length > 1;
  root.replaceChildren();
  for (const category of categories.values()) {
    const section = document.createElement('section');
    section.className = 'release-notes-category';
    const heading = document.createElement('h3');
    heading.textContent = category.title;
    section.append(heading);

    let list: HTMLOListElement | HTMLUListElement | undefined;
    let ordered: boolean | undefined;
    for (const entry of category.entries) {
      const tokens = marked.lexer(entry.markdown, { gfm: true });
      for (const token of tokens) {
        if (token.type === 'space') {
          continue;
        }
        if (token.type === 'list') {
          if (!list || ordered !== token.ordered) {
            list = document.createElement(token.ordered ? 'ol' : 'ul');
            ordered = token.ordered;
            section.append(list);
          }
          for (const item of token.items) {
            const row = document.createElement('li');
            appendInlineTokens(row, item.tokens);
            if (showVersion) {
              row.append(versionBadge(entry.version));
            }
            list.append(row);
          }
          continue;
        }
        list = undefined;
        const block = document.createElement(token.type === 'paragraph' ? 'p' : 'div');
        block.className = 'release-notes-change';
        if (token.type === 'paragraph') {
          appendInlineTokens(block, token.tokens ?? []);
        } else if (token.type === 'code') {
          const pre = document.createElement('pre');
          const code = document.createElement('code');
          code.textContent = token.text;
          pre.append(code);
          block.append(pre);
        } else if (token.type === 'blockquote') {
          const quote = document.createElement('blockquote');
          quote.innerHTML = clean(marked.parser(token.tokens ?? [], { gfm: true }));
          keepHttpsLinks(quote);
          block.append(quote);
        } else {
          block.textContent = 'text' in token ? token.text : token.raw;
        }
        if (showVersion) {
          block.append(versionBadge(entry.version));
        }
        section.append(block);
      }
    }
    root.append(section);
  }

  root.onclick = (event) => {
    const link = (event.target as Element | null)?.closest<HTMLAnchorElement>('a[href]');
    if (!link || !/^https:\/\//i.test(link.href)) {
      return;
    }
    event.preventDefault();
    openLink(link.href);
  };
}

function appendInlineTokens(root: Node, tokens: Token[]) {
  for (const token of tokens) {
    if (token.type === 'text' || token.type === 'escape') {
      if ('tokens' in token && token.tokens) {
        appendInlineTokens(root, token.tokens);
      } else {
        root.appendChild(document.createTextNode(token.text));
      }
      continue;
    }
    if (token.type === 'strong' || token.type === 'em' || token.type === 'del') {
      const element = document.createElement(token.type === 'strong' ? 'strong' : token.type);
      appendInlineTokens(element, token.tokens ?? []);
      root.appendChild(element);
      continue;
    }
    if (token.type === 'codespan') {
      const code = document.createElement('code');
      code.textContent = token.text;
      root.appendChild(code);
      continue;
    }
    if (token.type === 'link') {
      if (!token.href.startsWith('https://')) {
        root.appendChild(document.createTextNode(token.text));
      } else {
        const link = document.createElement('a');
        link.href = token.href;
        appendInlineTokens(link, token.tokens ?? []);
        root.appendChild(link);
      }
      continue;
    }
    if (token.type === 'br') {
      root.appendChild(document.createElement('br'));
      continue;
    }
    if (token.type === 'list') {
      const list = document.createElement(token.ordered ? 'ol' : 'ul');
      for (const item of token.items) {
        const row = document.createElement('li');
        appendInlineTokens(row, item.tokens);
        list.append(row);
      }
      root.appendChild(list);
      continue;
    }
    if (token.type === 'image') {
      root.appendChild(document.createTextNode(token.text));
    }
  }
}

function keepHttpsLinks(root: ParentNode) {
  for (const link of root.querySelectorAll<HTMLAnchorElement>('a')) {
    const href = link.getAttribute('href') ?? '';
    if (!href.startsWith('https://')) {
      link.replaceWith(link.textContent ?? '');
    }
  }
}

function categoriesFor(release: ReleaseNote, language: 'en' | 'zh', otherLabel: string) {
  const categories: Category[] = [];
  let current: Category | undefined;
  for (const token of marked.lexer(localizedChangelog(release.body, language), { gfm: true })) {
    if (token.type === 'heading') {
      if (token.depth === 1) {
        continue;
      }
      if (token.depth === 2) {
        current = { title: token.text.trim(), entries: [] };
        categories.push(current);
        continue;
      }
    }
    if (token.type === 'space') {
      continue;
    }
    if (!current) {
      current = { title: otherLabel, entries: [] };
      categories.push(current);
    }
    current.entries.push({ version: release.version, markdown: token.raw });
  }
  return categories.filter((category) => category.entries.length > 0);
}

function versionBadge(version: string) {
  const badge = document.createElement('span');
  badge.className = 'release-version';
  badge.textContent = `v${version.replace(/^v/, '')}`;
  return badge;
}
