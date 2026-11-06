import { describe, expect, test } from 'vite-plus/test';

import {
  CHINESE_MARKER,
  composeNotes,
  generateBilingualNotes,
  generateEnglishChangelog,
  translateToChinese,
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

  test('generateEnglishChangelog rejects a missing key without reading Git', async () => {
    await expect(
      generateEnglishChangelog({
        tag: 'v0.2.0',
        apiKey: '',
        runGit: () => {
          throw new Error('git should not run');
        },
      }),
    ).rejects.toThrow(/DEEPSEEK_API_KEY is required/);
  });

  test('generateEnglishChangelog asks DeepSeek to write from the previous release', async () => {
    let request;
    const changelog = await generateEnglishChangelog({
      tag: 'v0.3.0',
      previousTag: 'v0.2.0',
      apiKey: 'key',
      runGit: async (args) => {
        expect(args).toEqual(['log', 'v0.2.0..v0.3.0', '--pretty=format:%s', '--no-merges']);
        return 'feat: add release notes\nfix: keep the title';
      },
      fetchImpl: async (url, init) => {
        request = { url, body: JSON.parse(init.body) };
        return deepSeekReply('## Features\n\n- Add release notes');
      },
    });

    expect(changelog).toBe('## Features\n\n- Add release notes');
    expect(request.url).toBe('https://api.deepseek.com/chat/completions');
    expect(request.body.model).toBe('deepseek-flash');
    expect(request.body.messages).toEqual([
      {
        role: 'user',
        content:
          'Based on the following git commit messages, generate a concise and user-friendly changelog in markdown format. Start directly with the category sections. Use only these category headings, in this order and with this exact text:\n## Features\n## Improvements\n## Bug Fixes\nOmit a category entirely when it has no changes. Do not create any other category. Use exactly `##` (level 2) for every category heading. Do not add a document title, a `# Changelog` or `## Changelog` heading, and do not use `#` or `###` headings anywhere. Only include meaningful changes, skip trivial commits. Use clear, non-technical language where possible. Output ONLY the changelog content without any preamble. Do not add installation instructions or contributor acknowledgments; these are appended separately.\n\nCommit messages:\nfeat: add release notes\nfix: keep the title',
      },
    ]);
  });

  test('generateEnglishChangelog includes all history when no release has been published', async () => {
    let logArgs;
    await generateEnglishChangelog({
      tag: 'v0.1.1-beta.0',
      apiKey: 'key',
      runGit: async (args) => {
        expect(args[0]).toBe('log');
        logArgs = args;
        return 'feat: first release';
      },
      fetchImpl: async () => deepSeekReply('## Features\n\n- First release'),
    });

    expect(logArgs).toEqual(['log', 'v0.1.1-beta.0', '--pretty=format:%s', '--no-merges']);
  });

  test('generateEnglishChangelog fails without commits or a changelog', async () => {
    await expect(
      generateEnglishChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => '  \n',
        fetchImpl: () => {
          throw new Error('fetch should not run');
        },
      }),
    ).rejects.toThrow(/release commit list is required/);

    await expect(
      generateEnglishChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => 'fix: it',
        fetchImpl: async () => deepSeekReply(''),
      }),
    ).rejects.toThrow(/DeepSeek changelog is required/);
  });

  test('generateEnglishChangelog fails on a DeepSeek API error', async () => {
    await expect(
      generateEnglishChangelog({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => 'fix: it',
        fetchImpl: async () => new Response('busy', { status: 503 }),
      }),
    ).rejects.toThrow(/DeepSeek changelog returned HTTP 503/);
  });

  test('translateToChinese rejects missing keys and empty model output', async () => {
    await expect(
      translateToChinese('English', '', () => {
        throw new Error('fetch should not run');
      }),
    ).rejects.toThrow(/DEEPSEEK_API_KEY is required/);
    await expect(
      translateToChinese('English', 'key', async () => deepSeekReply('')),
    ).rejects.toThrow(/DeepSeek translation is required/);
  });

  test('translateToChinese fails on a DeepSeek API error', async () => {
    await expect(
      translateToChinese('English', 'key', async () => new Response('busy', { status: 503 })),
    ).rejects.toThrow(/DeepSeek translation returned HTTP 503/);
  });

  test('translateToChinese sends the generated notes to DeepSeek', async () => {
    let request;
    const chinese = await translateToChinese('English notes', 'key', async (url, init) => {
      request = { url, body: JSON.parse(init.body) };
      return deepSeekReply('中文说明');
    });

    expect(chinese).toBe('中文说明');
    expect(request.url).toBe('https://api.deepseek.com/chat/completions');
    expect(request.body.model).toBe('deepseek-flash');
    expect(request.body.messages).toEqual([
      {
        role: 'user',
        content:
          'Translate the following release notes into Simplified Chinese. Keep the markdown structure, links, code spans and product names (aidget, Codex, Claude Code, ...) as they are. Translate category headings to exactly:\n## 新功能\n## 改进\n## 错误修复\nUse only these category headings, in this order. Omit a category entirely when the source has no corresponding category, and do not create or translate any other category heading. Output ONLY the translation without any preamble.\n\nEnglish notes',
      },
    ]);
  });

  test('generateBilingualNotes generates English before translating it', async () => {
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
        expect(args).toEqual(['log', 'v0.1.0..v0.2.0', '--pretty=format:%s', '--no-merges']);
        return 'feat: add it';
      },
      fetchImpl: async (_url, init) => {
        const prompt = JSON.parse(init.body).messages[0].content;
        requests.push(prompt);
        return deepSeekReply(
          requests.length === 1 ? '## Features\n\n- Add it' : '## 新功能\n\n- 新增',
        );
      },
    });

    expect(requests).toHaveLength(2);
    expect(requests[0]).toMatch(/^Based on the following git commit messages/);
    expect(requests[1]).toMatch(/^Translate the following release notes/);
    expect(notes).toContain(`## Features\n\n- Add it`);
    expect(notes).toContain(CHINESE_MARKER);
    expect(notes).toContain(`## 新功能\n\n- 新增`);
    expect(notes).toContain('### Installation');
    expect(notes).toContain('### 安装说明');
    expect(requests[1]).not.toContain('xattr');
  });

  test('a contributor lookup failure stops before any paid model request', async () => {
    let requests = 0;
    await expect(
      generateBilingualNotes({
        tag: 'v0.2.0',
        apiKey: 'key',
        runGit: async () => 'fix: update',
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
});
