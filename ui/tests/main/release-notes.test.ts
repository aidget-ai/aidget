/// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vite-plus/test';

import { localizedChangelog, renderReleaseNotes } from '../../src/main/release-notes';
import type { UpdateNotes } from '../../src/main/update';

const body = `# Changelog

## Features

- English feature.

## Bug Fixes

- English fix.

### Installation

Install instructions.

### Acknowledgments

Thanks.

<!-- lang:zh -->

# Changelog

## 新功能

- 中文功能。

### 安装说明

安装说明。

### 致谢

感谢。
`;

beforeEach(() => {
  document.body.replaceChildren();
});

describe('release-note language selection', () => {
  it('selects only the changelog in the requested language', () => {
    expect(localizedChangelog(body, 'en-US')).toContain('English feature.');
    expect(localizedChangelog(body, 'en-US')).toContain('English fix.');
    expect(localizedChangelog(body, 'en-US')).not.toContain('Install instructions.');
    expect(localizedChangelog(body, 'zh-CN')).toContain('中文功能。');
    expect(localizedChangelog(body, 'zh-CN')).not.toContain('安装说明。');
  });

  it('fails explicitly when the bilingual separator is absent', () => {
    expect(() => localizedChangelog('# Changelog\n\n- only one language', 'zh-CN')).toThrow(
      'Chinese-language separator',
    );
  });
});

describe('release-note rendering', () => {
  it('omits the source version when notes contain a single release', () => {
    const root = document.createElement('div');
    document.body.append(root);

    renderReleaseNotes(
      root,
      {
        from: '1.0.0',
        to: '1.1.0',
        releases: [
          {
            version: '1.1.0',
            body: `# Changelog

## Features

- Only change.

<!-- lang:zh -->

# Changelog

## 新功能

### 安装说明
`,
          },
        ],
      },
      'en-US',
      'Other changes',
      () => {},
    );

    expect(root.querySelector('.release-version')).toBeNull();
  });

  it('merges categories and adds the source version to every change', () => {
    const root = document.createElement('div');
    document.body.append(root);
    const notes: UpdateNotes = {
      from: 'v1.0.0',
      to: 'v1.2.0',
      releases: [
        {
          version: '1.2.0',
          body: `# Changelog

## Features

- Newest feature.

## Bug Fixes

- Newest fix.

<!-- lang:zh -->

# Changelog

## 新功能

- 最新功能。

### 安装说明
`,
        },
        {
          version: '1.1.0',
          body: `# Changelog

## Features

- Older feature.

<!-- lang:zh -->

# Changelog

## 新功能

- 较早功能。

### 安装说明
`,
        },
      ],
    };

    renderReleaseNotes(root, notes, 'en-US', 'Other changes', () => {});

    expect([...root.querySelectorAll('h3')].map((heading) => heading.textContent)).toEqual([
      'Features',
      'Bug Fixes',
    ]);
    const featureItems = [...root.querySelectorAll('.release-notes-category:first-child li')];
    expect(featureItems.map((item) => item.textContent)).toEqual([
      'Newest feature.v1.2.0',
      'Older feature.v1.1.0',
    ]);
  });

  it('sanitizes raw HTML and non-HTTPS links', () => {
    const root = document.createElement('div');
    document.body.append(root);

    renderReleaseNotes(
      root,
      {
        from: '1.0.0',
        to: '1.1.0',
        releases: [
          {
            version: '1.1.0',
            body: `# Changelog

## Security

<img src=x onerror="alert(1)">

- [Unsafe](javascript:alert(1))
- [Safe](https://example.com/release)
<script>alert(1)</script>

<!-- lang:zh -->

# Changelog

## 安全

### 安装说明
`,
          },
        ],
      },
      'en-US',
      'Other changes',
      () => {},
    );

    expect(root.querySelector('img')).toBeNull();
    expect(root.querySelector('script')).toBeNull();
    expect(root.querySelector('a[href^="javascript"]')).toBeNull();
    expect(
      root.querySelector<HTMLAnchorElement>('a[href="https://example.com/release"]'),
    ).toBeTruthy();
  });

  it('opens sanitized HTTPS links through the supplied handler', () => {
    const root = document.createElement('div');
    document.body.append(root);
    const open = vi.fn();
    renderReleaseNotes(
      root,
      {
        from: '1.0.0',
        to: '1.1.0',
        releases: [
          {
            version: '1.1.0',
            body: `# Changelog

## Links

- [Release](https://example.com/release)

<!-- lang:zh -->

# Changelog

## 链接

### 安装说明
`,
          },
        ],
      },
      'en-US',
      'Other changes',
      open,
    );

    root
      .querySelector<HTMLAnchorElement>('a')!
      .dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(open).toHaveBeenCalledWith('https://example.com/release');
  });
});
