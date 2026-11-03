import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import { runGit } from '../utils/git.mjs';

const execFileAsync = promisify(execFile);
const MAINTAINERS = new Set(['micooz']);
const BOT =
  /\[bot\]$|^(claude(?: code| opus.*| sonnet.*| haiku.*)?|amp|codex|copilot|github copilot|cursor(?: agent)?|devin(?: ai)?|gemini|opencode|droid|factory-droid|dependabot|github-actions|renovate|kluster)$/i;
const AGENT =
  /^(?:admin|root|user|ubuntu|runner|test)$|\bagent$|^(?:claude|gpt|glm|qwen|kimi|deepseek|gemini|grok|minimax|doubao|o\d)\b.*\d/i;
const AGENT_MAIL =
  /@(anthropic\.com|cursor\.com|openai\.com|instinct\.com|multica\.ai|factory\.ai|ampcode\.com|google\.com|localhost)$|^(noreply|agent|bot)@/i;
const PLATFORMS =
  'Discord|GitHub|X|Twitter|V2EX|WeChat|微信|Telegram|Reddit|LINUX\\.DO|Jike|即刻|小红书';
const STOP = new Set(
  'cache codex discord github linux macos windows api cli gui ci p0 p1 p2 p3 fix test tests beta alpha stable'.split(
    ' ',
  ),
);

function githubLogin(email) {
  return email.match(/^(?:\d+\+)?([a-z\d-]+)@users\.noreply\.github\.com$/i)?.[1] || '';
}

function human(name, email = '') {
  return (
    !!name &&
    !/\S+@\S+/.test(name) &&
    !BOT.test(name) &&
    !AGENT.test(name) &&
    !AGENT_MAIL.test(email)
  );
}

async function readIssue(repository, number) {
  const { stdout } = await execFileAsync(
    'gh',
    [
      'api',
      `repos/${repository}/issues/${number}`,
      '--jq',
      '.user | if . == null then null else {login, type} end',
    ],
    { encoding: 'utf8', timeout: 30_000, maxBuffer: 1024 * 1024 },
  );
  const author = JSON.parse(stdout);
  if (
    author !== null &&
    (typeof author.login !== 'string' || !['User', 'Bot'].includes(author.type))
  ) {
    throw new Error(`${repository}#${number}: invalid GitHub author response`);
  }
  return author;
}

/** Collect public people credited by non-merge commits in the changelog range. */
export async function collectContributors({
  range,
  repository = 'aidget-ai/aidget',
  runGit: git = runGit,
  readIssue: issue = readIssue,
}) {
  if (!range || !/^[\w.-]+\/[\w.-]+$/.test(repository)) {
    throw new Error('contributors require a Git range and owner/repository');
  }
  const output = await git([
    'log',
    range,
    '--no-merges',
    '--format=%H%x1f%an%x1f%ae%x1f%s%x1f%B%x1e',
  ]);
  const commits = output
    .split('\x1e')
    .filter((record) => record.trim())
    .map((record) => {
      const fields = record.replace(/^\n/, '').split('\x1f');
      if (fields.length !== 5) {
        throw new Error('malformed contributor Git log record');
      }
      const [, name, email, subject, body] = fields;
      const coauthors = [...body.matchAll(/^Co-authored-by:\s*(.*?)\s*<([^>]+)>\s*$/gim)].map(
        (match) => ({ name: match[1].trim(), email: match[2].trim() }),
      );
      return { name, email, subject, body, coauthors };
    });
  const people = new Map();
  const add = (name, github = '') => {
    name = name.trim();
    if (
      !human(name) ||
      (github && !human(github)) ||
      MAINTAINERS.has(name.toLowerCase()) ||
      MAINTAINERS.has(github.toLowerCase())
    ) {
      return;
    }
    const key = (github || name).toLowerCase();
    if (!people.has(key)) {
      people.set(key, { name, github });
    }
  };
  for (const commit of commits) {
    for (const author of [commit, ...commit.coauthors]) {
      if (human(author.name, author.email)) {
        add(author.name, githubLogin(author.email));
      }
    }
  }

  // Resolve each referenced issue or PR once; failed lookups abort the release.
  const refs = new Map();
  for (const { body } of commits) {
    for (const match of body.matchAll(/(?:([\w.-]+\/[\w.-]+|[\w.-]+))?#([1-9]\d*)\b/g)) {
      let repo = match[1] || repository;
      if (repo === 'widgets') {
        repo = `${repository.split('/')[0]}/widgets`;
      }
      if (repo === repository.split('/')[1]) {
        repo = repository;
      }
      if (repo.includes('/')) {
        refs.set(`${repo}#${match[2]}`, { repo, number: Number(match[2]) });
      }
    }
  }
  for (const { repo, number } of refs.values()) {
    const author = await issue(repo, number);
    if (author?.type === 'User') {
      add(author.login, author.login);
    }
  }

  const uncertain = [];
  for (const { subject } of commits) {
    for (const match of subject.matchAll(
      /\b(?:spotted|reported|suggested|requested|found|tested) by @([a-z\d-]+)\b/gi,
    )) {
      add(match[1], match[1]);
    }
    for (const group of subject.matchAll(/\(([^()]*)\)/g)) {
      for (const part of group[1].split(';')) {
        const match = part
          .trim()
          .match(new RegExp(`^(.+?)(?:\\s+on\\s+|,\\s*)(${PLATFORMS})(?:\\s*[:：].*)?$`, 'i'));
        if (match) {
          for (const value of match[1].split(/,\s*|\s+(?:and|&)\s+/)) {
            const name = value.trim().replace(/^@/, '');
            if (name && name.length <= 80 && !/[#<>]/.test(name)) {
              add(name, /^github$/i.test(match[2]) && /^[a-z\d-]+$/i.test(name) ? name : '');
            }
          }
        } else {
          for (const word of part.split(/,\s*/)) {
            uncertain.push(word.trim().replace(/^@/, ''));
          }
        }
      }
    }
  }
  const known = new Set(
    [...people.values()].flatMap(({ name, github }) => [name.toLowerCase(), github.toLowerCase()]),
  );
  for (const name of uncertain) {
    if (STOP.has(name.toLowerCase()) || known.has(name.toLowerCase())) {
      continue;
    }
    if (/^[a-z]{3,}\d{2,}$/i.test(name) || /^_?[a-z][\w]*_\d{3,}$/i.test(name)) {
      add(name);
    }
  }
  return [...people.values()].sort((a, b) =>
    a.name.localeCompare(b.name, 'en', { sensitivity: 'base' }),
  );
}

/** Render the same public contributor names at the end of each language section. */
export function thanks(people, language) {
  if (!['en', 'zh'].includes(language)) {
    throw new Error(`unsupported release notes language ${language}`);
  }
  if (!people.length) {
    return '';
  }
  const names = people.map(({ name, github }) => {
    const escaped = name.replace(/([\\`*_{}[\]<>()#+!|~])/g, '\\$1');
    return github ? `[${escaped}](https://github.com/${encodeURIComponent(github)})` : escaped;
  });
  return language === 'zh'
    ? `### 致谢\n\n感谢 ${names.join('、')}：这次更新包含了你们的反馈或代码。`
    : `### Acknowledgments\n\nThanks to ${names.join(', ')}: this update includes your feedback or code.`;
}
