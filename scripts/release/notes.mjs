import { writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

import { runGit } from '../utils/git.mjs';
import { collectContributors, thanks } from './contributors.mjs';
import { installationNotes } from './install-notes.mjs';
import { parseTag } from './version.mjs';

export const CHINESE_MARKER = '<!-- lang:zh -->';

function required(value, name) {
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function responseError(label, status, body) {
  return new Error(`${label} returned HTTP ${status}: ${body.slice(0, 500)}`);
}

// One DeepSeek call: send the prompt and require non-empty text in return.
async function deepSeekText(prompt, apiKey, fetchImpl, label) {
  required(apiKey, 'DEEPSEEK_API_KEY');

  const response = await fetchImpl('https://api.deepseek.com/chat/completions', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      model: 'deepseek-flash',
      messages: [{ role: 'user', content: prompt }],
    }),
  });
  if (!response.ok) {
    throw responseError(`DeepSeek ${label}`, response.status, await response.text());
  }

  const data = await response.json();
  const text = data?.choices?.[0]?.message?.content;
  return required(typeof text === 'string' ? text.trim() : '', `DeepSeek ${label}`);
}

/** Append fixed localized footers and join the two language sections. */
export function composeNotes(english, chinese, people = []) {
  const en = required(english.trim(), 'English release notes');
  const zh = required(chinese.trim(), 'Chinese release notes');
  return (
    [
      en,
      installationNotes('en'),
      thanks(people, 'en'),
      CHINESE_MARKER,
      zh,
      installationNotes('zh'),
      thanks(people, 'zh'),
    ]
      .filter(Boolean)
      .join('\n\n') + '\n'
  );
}

/** Generate English notes since the last published stable tag, or from all history. */
export async function generateEnglishChangelog({
  tag,
  previousTag = '',
  apiKey,
  fetchImpl = fetch,
  runGit: git = runGit,
}) {
  required(tag, 'RELEASE_TAG');
  required(apiKey, 'DEEPSEEK_API_KEY');
  parseTag(tag);
  if (previousTag) {
    parseTag(previousTag);
  }

  // Failed and unpublished tags must not truncate the first release's history.
  const range = previousTag ? `${previousTag}..${tag}` : tag;
  const commits = required(
    (await git(['log', range, '--pretty=format:%s', '--no-merges'])).trim(),
    'release commit list',
  );
  const prompt = `Based on the following git commit messages, generate a concise and user-friendly changelog in markdown format. Start directly with the category sections. Use exactly \`##\` (level 2) for every category heading, for example \`## Features\` and \`## Bug Fixes\`. Do not add a document title, a \`# Changelog\` or \`## Changelog\` heading, and do not use \`#\` or \`###\` headings anywhere. Group changes by category (e.g., Features, Bug Fixes, Improvements). Only include meaningful changes, skip trivial commits. Use clear, non-technical language where possible. Output ONLY the changelog content without any preamble. Do not add installation instructions or contributor acknowledgments; these are appended separately.

Commit messages:
${commits}`;
  return deepSeekText(prompt, apiKey, fetchImpl, 'changelog');
}

/** Translates generated English notes into Simplified Chinese. */
export async function translateToChinese(english, apiKey, fetchImpl = fetch) {
  required(apiKey, 'DEEPSEEK_API_KEY');

  const prompt = `Translate the following release notes into Simplified Chinese. Keep the markdown structure, headings, links, code spans and product names (aidget, Codex, Claude Code, ...) as they are. Output ONLY the translation without any preamble.

${english}`;
  return deepSeekText(prompt, apiKey, fetchImpl, 'translation');
}

/** Generate and translate the changelog, then append fixed installation notes and credits. */
export async function generateBilingualNotes({
  tag,
  previousTag = '',
  apiKey,
  fetchImpl = fetch,
  runGit: git = runGit,
  repository = 'aidget-ai/aidget',
  collectContributors: contributors = collectContributors,
}) {
  required(apiKey, 'DEEPSEEK_API_KEY');
  parseTag(tag);
  if (previousTag) {
    parseTag(previousTag);
  }
  // Resolve credits first so a GitHub failure does not spend the model API budget.
  const people = await contributors({
    range: previousTag ? `${previousTag}..${tag}` : tag,
    repository,
    runGit: git,
  });
  const english = await generateEnglishChangelog({
    tag,
    previousTag,
    apiKey,
    fetchImpl,
    runGit: git,
  });
  const chinese = await translateToChinese(english, apiKey, fetchImpl);
  return composeNotes(english, chinese, people);
}

async function main(argv) {
  const outIndex = argv.indexOf('--out');
  const out = outIndex === -1 ? 'release-notes.md' : argv[outIndex + 1];
  if (!out) {
    throw new Error('--out needs a file path');
  }

  const tag = required(process.env.RELEASE_TAG, 'RELEASE_TAG');
  const notes = await generateBilingualNotes({
    tag,
    previousTag: process.env.PREVIOUS_RELEASE_TAG,
    apiKey: process.env.DEEPSEEK_API_KEY,
    repository: process.env.GITHUB_REPOSITORY,
  });
  await writeFile(out, notes);
  console.log(`wrote ${out}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`release notes: ${error.message}`);
    process.exitCode = 1;
  });
}
