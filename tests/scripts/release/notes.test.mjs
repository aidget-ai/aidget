import { describe, expect, test } from 'vite-plus/test';

import {
  CHINESE_MARKER,
  composeNotes,
  generateBilingualChangelog,
  generateBilingualNotes,
  INTERNAL_ONLY_MARKER,
  splitBilingualNotes,
} from '../../../scripts/release/notes.mjs';

function deepSeekReply(content) {
  return new Response(JSON.stringify({ choices: [{ message: { content } }] }), {
    status: 200,
  });
}

describe('release notes', () => {
  test('composeNotes keeps localized instructions and thanks after each changelog', () => {
    const people = [{ name: 'Alice', github: 'alice' }];
    const [en, zh] = composeNotes('English changes', '中文更新', people).split(CHINESE_MARKER);
    expect(en.indexOf('English changes')).toBeLessThan(en.indexOf('### Installation'));
    expect(en.indexOf('### Installation')).toBeLessThan(en.indexOf('### Acknowledgments'));
    expect(zh.indexOf('中文更新')).toBeLessThan(zh.indexOf('### 安装说明'));
    expect(zh.indexOf('### 安装说明')).toBeLessThan(zh.indexOf('### 致谢'));
    expect(en.trim()).toMatch(/code\.$/);
    expect(zh.trim()).toMatch(/代码。$/);
    for (const part of [en, zh]) {
      expect(part).toContain('Alice');
    }
    expect(() => composeNotes('', '中文')).toThrow();
  });

  test('generateBilingualChangelog rejects a missing key without reading Git', async () => {
    await expect(
      generateBilingualChangelog({
        tag: 'v0.2.0',
        apiKey: '',
        runGit: () => {
          throw new Error('git should not run');
        },
      }),
    ).rejects.toThrow(/DEEPSEEK_API_KEY is required/);
  });

  test('generateBilingualChangelog requests both languages in one Markdown response', async () => {
    let request;
    const bilingual = [
      '## Features',
      '- Add release notes',
      '',
      CHINESE_MARKER,
      '',
      '## 新功能',
      '- 新增发布说明',
    ].join('\n');
    const changelog = await generateBilingualChangelog({
      tag: 'v0.3.0',
      previousTag: 'v0.2.0',
      apiKey: 'key',
      runGit: async (args) => {
        expect(args).toEqual([
          'log',
          'v0.2.0..v0.3.0',
          '--pretty=format:### %h%n**Subject:** %s%n%n%b',
          '--no-merges',
        ]);
        return [
          '### abc1234',
          '**Subject:** feat: add release notes',
          '',
          '- add the release-notes workflow',
          '',
          '### def5678',
          '**Subject:** fix: keep the title',
          '',
          '- preserve an existing title',
        ].join('\n');
      },
      fetchImpl: async (url, init) => {
        request = { url, body: JSON.parse(init.body) };
        return deepSeekReply(bilingual);
      },
    });

    expect(changelog).toBe(bilingual);
    expect(request.url).toBe('https://api.deepseek.com/chat/completions');
    expect(request.body.model).toBe('deepseek-flash');
    expect(request.body.messages).toHaveLength(1);
    expect(request.body.messages[0].role).toBe('user');
    const prompt = request.body.messages[0].content;
    expect(prompt).toContain('Generate English and Simplified Chinese release notes together');
    expect(prompt).toContain('one bullet for one bullet');
    expect(prompt).toContain(
      'After the complete English section, output this exact separator exactly once',
    );
    expect(prompt).toContain(CHINESE_MARKER);
    expect(prompt).toContain('## Features\n## Improvements\n## Bug Fixes');
    expect(prompt).toContain('## 新功能\n## 改进\n## 错误修复');
    expect(prompt).toContain('preserve links, code spans, product names');
    expect(prompt).toContain('Omit a section entirely when it has no changes');
    expect(prompt).toContain('Do not copy a vague subject verbatim');
    expect(prompt).toContain('not the mechanism behind it');
    expect(prompt).toContain(INTERNAL_ONLY_MARKER);
    expect(prompt).toContain('This release contains internal maintenance only');
    expect(prompt).toContain('本次更新仅内部维护');
    expect(prompt).toContain('# Release context');
    expect(prompt).toContain('- Release: v0.3.0');
    expect(prompt).toContain('# Commits');
    expect(prompt).toContain('### abc1234');
    expect(prompt).toContain('- preserve an existing title');
    expect(prompt).not.toContain('"features"');
  });

  test('generateBilingualChangelog includes all history when no release has been published', async () => {
    let logArgs;
    await generateBilingualChangelog({
      tag: 'v0.1.1-beta.0',
      apiKey: 'key',
      runGit: async (args) => {
        expect(args[0]).toBe('log');
        logArgs = args;
        return '### abc1234\n**Subject:** feat: first release';
      },
      fetchImpl: async () =>
        deepSeekReply(`## Features\n- First release\n\n${CHINESE_MARKER}\n\n## 新功能\n- 首次发布`),
    });

    expect(logArgs).toEqual([
      'log',
      'v0.1.1-beta.0',
      '--pretty=format:### %h%n**Subject:** %s%n%n%b',
      '--no-merges',
    ]);
  });

  test('generateBilingualChangelog fails without commits or a changelog', async () => {
    await expect(
      generateBilingualChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => '  \n',
        fetchImpl: () => {
          throw new Error('fetch should not run');
        },
      }),
    ).rejects.toThrow(/release commit list is required/);

    await expect(
      generateBilingualChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => '### abc1234\n**Subject:** fix: it',
        fetchImpl: async () => deepSeekReply(''),
      }),
    ).rejects.toThrow(/DeepSeek bilingual changelog is required/);
  });

  test('generateBilingualChangelog fails on a DeepSeek API error', async () => {
    await expect(
      generateBilingualChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => '### abc1234\n**Subject:** fix: it',
        fetchImpl: async () => new Response('busy', { status: 503 }),
      }),
    ).rejects.toThrow(/DeepSeek bilingual changelog returned HTTP 503/);
  });

  test('splitBilingualNotes requires one marker and matching categories', () => {
    const bilingual = [
      '## Features',
      '- Add it',
      '',
      '## Bug Fixes',
      '- Fix it',
      '',
      CHINESE_MARKER,
      '',
      '## 新功能',
      '- 新增',
      '',
      '## 错误修复',
      '- 修复',
    ].join('\n');
    expect(splitBilingualNotes(bilingual)).toEqual([
      '## Features\n- Add it\n\n## Bug Fixes\n- Fix it',
      '## 新功能\n- 新增\n\n## 错误修复\n- 修复',
    ]);
    expect(() => splitBilingualNotes('## Features\n- Add it')).toThrow(/exactly one/);
    expect(() => splitBilingualNotes(`${bilingual}\n${CHINESE_MARKER}\n`)).toThrow(/exactly one/);
    expect(() => splitBilingualNotes(`${CHINESE_MARKER}\n## 新功能\n- 新增`)).toThrow(
      /changelog section is required/,
    );
    expect(() =>
      splitBilingualNotes(`## Features\n- Add it\n\n${CHINESE_MARKER}\n\n## 改进\n- 新增`),
    ).toThrow(/categories must correspond/);
    expect(() =>
      splitBilingualNotes(
        `## Features\n- Add it\n\n## Bug Fixes\n- Fix it\n\n${CHINESE_MARKER}\n\n## 新功能\n- 新增`,
      ),
    ).toThrow(/categories must correspond/);
    expect(() =>
      splitBilingualNotes(`## Changes\n- Add it\n\n${CHINESE_MARKER}\n\n## 改进\n- 新增`),
    ).toThrow(/must use only these level-2 headings/);
  });

  test('splitBilingualNotes accepts the exact no-user-visible-change result', () => {
    const english = `${INTERNAL_ONLY_MARKER}

This release contains internal maintenance only; app features and behavior remain unchanged.`;
    const chinese = '本次更新仅内部维护，应用功能和使用方式保持不变。';
    expect(splitBilingualNotes(`${english}\n\n${CHINESE_MARKER}\n\n${chinese}`)).toEqual([
      english,
      chinese,
    ]);
    expect(() => splitBilingualNotes(`${english}\n\n${CHINESE_MARKER}\n\n没有变化`)).toThrow(
      /must use only these level-2 headings/,
    );
  });

  test('generateBilingualNotes uses one model call and appends localized footers', async () => {
    const requests = [];
    const notes = await generateBilingualNotes({
      tag: 'v0.2.0',
      previousTag: 'v0.1.0',
      apiKey: 'key',
      collectContributors: async ({ range }) => {
        expect(range).toBe('v0.1.0..v0.2.0');
        return [];
      },
      runGit: async (args) => {
        expect(args).toEqual([
          'log',
          'v0.1.0..v0.2.0',
          '--pretty=format:### %h%n**Subject:** %s%n%n%b',
          '--no-merges',
        ]);
        return '### abc1234\n**Subject:** feat: add it\n\n- add the user-visible behavior';
      },
      fetchImpl: async (_url, init) => {
        const prompt = JSON.parse(init.body).messages[0].content;
        requests.push(prompt);
        return deepSeekReply(`## Features\n\n- Add it\n\n${CHINESE_MARKER}\n\n## 新功能\n\n- 新增`);
      },
    });

    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatch(/^Generate English and Simplified Chinese release notes together/);
    expect(notes).toContain(`## Features\n\n- Add it`);
    expect(notes).toContain(CHINESE_MARKER);
    expect(notes).toContain(`## 新功能\n\n- 新增`);
    expect(notes).toContain('### Installation');
    expect(notes).toContain('### 安装说明');
    expect(requests[0]).not.toContain('xattr');
  });

  test('a contributor lookup failure stops before any paid model request', async () => {
    let requests = 0;
    await expect(
      generateBilingualNotes({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => '### abc1234\n**Subject:** fix: update',
        collectContributors: async () => {
          throw new Error('GitHub HTTP 403');
        },
        fetchImpl: async () => {
          requests++;
          return deepSeekReply('notes');
        },
      }),
    ).rejects.toThrow('GitHub HTTP 403');
    expect(requests).toBe(0);
  });

  test('an internal-only release keeps the no-user-visible-change fallback', async () => {
    const notes = await generateBilingualNotes({
      tag: 'v0.2.0',
      previousTag: 'v0.1.0',
      apiKey: 'key',
      runGit: async () =>
        '### abc1234\n**Subject:** ci: refresh dependency cache\n\n- update the cache key',
      collectContributors: async () => [],
      fetchImpl: async () =>
        deepSeekReply(
          `${INTERNAL_ONLY_MARKER}\n\nThis release contains internal maintenance only; app features and behavior remain unchanged.\n\n${CHINESE_MARKER}\n\n本次更新仅内部维护，应用功能和使用方式保持不变。`,
        ),
    });

    expect(notes).toContain(INTERNAL_ONLY_MARKER);
    expect(notes).toContain('This release contains internal maintenance only');
    expect(notes).toContain('本次更新仅内部维护');
    expect(notes).toContain('### Installation');
    expect(notes).toContain('### 安装说明');
  });
});
