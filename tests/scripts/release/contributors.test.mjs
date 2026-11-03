import { describe, expect, test } from 'vite-plus/test';

import { collectContributors, thanks } from '../../../scripts/release/contributors.mjs';

function log(...commits) {
  return commits
    .map(
      ({ name, email = '', subject = '', body = '' }, i) =>
        [`sha${i}`, name, email, subject, `${subject}\n\n${body}`].join('\x1f') + '\x1e',
    )
    .join('\n');
}

describe('release contributors', () => {
  test('collects authors, coauthors, reporters and referenced issue/PR authors', async () => {
    const refs = [];
    const people = await collectContributors({
      range: 'v0.1.0..v0.2.0',
      runGit: async (args) => {
        expect(args).toContain('v0.1.0..v0.2.0');
        expect(args).toContain('--no-merges');
        return log(
          {
            name: 'Alice',
            email: '123+alice' + '@users.noreply.github.com',
            subject: 'fix: layout (Ben on Discord)',
            body: 'Refs #2\nCo-authored-by: Carol <' + 'carol' + '@users.noreply.github.com>',
          },
          {
            name: 'micooz',
            subject: 'fix: widgets (spotted by @dave)',
            body: 'Fixes widgets#3\nRelated other/repo#4\nRefs #2',
          },
        );
      },
      readIssue: async (repo, number) => {
        refs.push(`${repo}#${number}`);
        return { login: { 2: 'alice', 3: 'eve', 4: 'frank' }[number], type: 'User' };
      },
    });
    expect(people).toEqual([
      { name: 'Alice', github: 'alice' },
      { name: 'Ben', github: '' },
      { name: 'Carol', github: 'carol' },
      { name: 'dave', github: 'dave' },
      { name: 'eve', github: 'eve' },
      { name: 'frank', github: 'frank' },
    ]);
    expect(refs.sort((a, b) => a.localeCompare(b))).toEqual([
      'aidget-ai/aidget#2',
      'aidget-ai/widgets#3',
      'other/repo#4',
    ]);
  });

  test('excludes maintainers, agents, bots and unproven parenthesized code words', async () => {
    const people = await collectContributors({
      range: 'HEAD',
      runGit: async () =>
        log(
          {
            name: 'Micooz',
            subject: 'fix: cache (Codex) (P2) (cache)',
            body: 'Co-authored-by: Claude <agent@localhost>\nCo-authored-by: root <root@localhost>',
          },
          { name: 'kluster' },
          { name: 'github-actions[bot]' },
          { name: 'User', email: 'codex' + '@openai.com' },
          { name: 'A Real Person', email: 'private@example.org', subject: 'fix: tools (#7)' },
        ),
      readIssue: async () => ({ login: 'dependabot[bot]', type: 'Bot' }),
    });
    expect(people).toEqual([{ name: 'A Real Person', github: '' }]);
    expect(JSON.stringify(people)).not.toContain('private@example.org');
  });

  test('does not invent an identity for a deleted issue author', async () => {
    expect(
      await collectContributors({
        range: 'HEAD',
        runGit: async () => log({ name: 'micooz', subject: 'fix: thing (#1)' }),
        readIssue: async () => null,
      }),
    ).toEqual([]);
  });

  test('never publishes an email used as an author or reporter name', async () => {
    const address = 'private' + '@example.org';
    const people = await collectContributors({
      range: 'HEAD',
      runGit: async () => log({ name: address, subject: `fix: layout (${address} on Discord)` }),
    });
    expect(people).toEqual([]);
  });

  test('does not assign a GitHub identity based on a shared display name', async () => {
    const people = await collectContributors({
      range: 'HEAD',
      runGit: async () =>
        log(
          { name: 'Alex', email: 'alex' + '@example.org' },
          { name: 'Alex', email: 'another-alex' + '@users.noreply.github.com' },
        ),
    });
    expect(people).toEqual([
      { name: 'Alex', github: '' },
      { name: 'Alex', github: 'another-alex' },
    ]);
  });

  test('propagates GitHub failures instead of silently omitting people', async () => {
    await expect(
      collectContributors({
        range: 'HEAD',
        runGit: async () => log({ name: 'micooz', subject: 'fix: thing (#1)' }),
        readIssue: async () => {
          throw new Error('HTTP 403');
        },
      }),
    ).rejects.toThrow('HTTP 403');
  });

  test('escapes names and gives the same names to both languages', () => {
    const people = [
      { name: 'A_[B]', github: '' },
      { name: 'Carol', github: 'carol' },
    ];
    const en = thanks(people, 'en');
    const zh = thanks(people, 'zh');
    expect(en).toBe(
      '### Acknowledgments\n\nThanks to A\\_\\[B\\], Carol: this update includes your feedback or code.',
    );
    expect(zh).toBe('### 致谢\n\n感谢 A\\_\\[B\\]、Carol：这次更新包含了你们的反馈或代码。');
    for (const text of [en, zh]) {
      expect(text).toContain('A\\_\\[B\\]');
      expect(text).toContain('Carol');
      expect(text).not.toContain('https://');
    }
    expect(thanks([], 'en')).toBe('');
  });
});
