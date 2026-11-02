import { execFile } from 'node:child_process';
import { writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { promisify } from 'node:util';

import { parseTag } from './version.mjs';

export const CHINESE_MARKER = '<!-- lang:zh -->';

const execFileAsync = promisify(execFile);

function required(value, name) {
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function responseError(label, status, body) {
  return new Error(`${label} returned HTTP ${status}: ${body.slice(0, 500)}`);
}

async function runGit(args) {
  const { stdout } = await execFileAsync('git', args, {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
  return stdout;
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
      model: 'deepseek-v4-pro',
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

/** Joins the English and Chinese notes around the marker clients split on. */
export function composeNotes(english, chinese) {
  const en = required(english.trim(), 'English release notes');
  const zh = required(chinese.trim(), 'Chinese release notes');
  return `${en}\n\n${CHINESE_MARKER}\n\n${zh}\n`;
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
  const prompt = `Based on the following git commit messages, generate a concise and user-friendly changelog in markdown format. Group changes by category (e.g., Features, Bug Fixes, Improvements). Only include meaningful changes, skip trivial commits. Use clear, non-technical language where possible. Output ONLY the changelog content without any preamble.

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

/** Runs the changelog and translation calls in order and composes their result. */
export async function generateBilingualNotes({
  tag,
  previousTag = '',
  apiKey,
  fetchImpl = fetch,
  runGit: git = runGit,
}) {
  const english = await generateEnglishChangelog({
    tag,
    previousTag,
    apiKey,
    fetchImpl,
    runGit: git,
  });
  const chinese = await translateToChinese(english, apiKey, fetchImpl);
  return composeNotes(english, chinese);
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
