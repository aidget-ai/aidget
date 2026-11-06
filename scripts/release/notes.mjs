import { writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

import { runGit } from '../utils/git.mjs';
import { collectContributors, thanks } from './contributors.mjs';
import { installationNotes } from './install-notes.mjs';
import { parseTag } from './version.mjs';

export const CHINESE_MARKER = '<!-- lang:zh -->';
export const INTERNAL_ONLY_MARKER = '<!-- aidget:internal-only -->';
const COMMIT_LOG_FORMAT = '--pretty=format:### %h%n**Subject:** %s%n%n%b';
const CATEGORY_PAIRS = [
  ['Features', '新功能'],
  ['Improvements', '改进'],
  ['Bug Fixes', '错误修复'],
];
const MAINTENANCE_ONLY_MESSAGES = {
  english:
    'This release contains internal maintenance only; app features and behavior remain unchanged.',
  chinese: '本次更新仅内部维护，应用功能和使用方式保持不变。',
};

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

function categoryHeadings(markdown, labels) {
  const headings = [...markdown.matchAll(/^## (.+)$/gm)].map((match) => match[1].trim());
  const expected = labels.filter((label) => headings.includes(label));
  if (
    headings.length === 0 ||
    headings.length !== expected.length ||
    headings.some((heading, index) => heading !== expected[index])
  ) {
    throw new Error(
      `release notes must use only these level-2 headings in order: ${labels.join(', ')}`,
    );
  }
  return headings;
}

/** Split and validate the bilingual Markdown returned by one DeepSeek call. */
export function splitBilingualNotes(markdown) {
  const sections = required(markdown.trim(), 'DeepSeek bilingual changelog').split(CHINESE_MARKER);
  if (sections.length !== 2) {
    throw new Error(`DeepSeek changelog must contain exactly one ${CHINESE_MARKER} separator`);
  }
  const [english, chinese] = sections.map((section) =>
    required(section.trim(), 'changelog section'),
  );
  if (
    english === `${INTERNAL_ONLY_MARKER}\n\n${MAINTENANCE_ONLY_MESSAGES.english}` &&
    chinese === MAINTENANCE_ONLY_MESSAGES.chinese
  ) {
    return [english, chinese];
  }
  const englishHeadings = categoryHeadings(
    english,
    CATEGORY_PAIRS.map(([english]) => english),
  );
  const chineseHeadings = categoryHeadings(
    chinese,
    CATEGORY_PAIRS.map(([, chinese]) => chinese),
  );
  const expectedChinese = englishHeadings.map(
    (heading) => CATEGORY_PAIRS.find(([english]) => english === heading)[1],
  );
  if (
    chineseHeadings.length !== expectedChinese.length ||
    chineseHeadings.some((heading, index) => heading !== expectedChinese[index])
  ) {
    throw new Error('English and Chinese changelog categories must correspond');
  }
  return [english, chinese];
}

/** Generate bilingual notes since the last published stable tag, or from all history. */
export async function generateBilingualChangelog({
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
    (await git(['log', range, COMMIT_LOG_FORMAT, '--no-merges'])).trim(),
    'release commit list',
  );
  const prompt = `Generate English and Simplified Chinese release notes together from the Git commits below.

Use only facts explicitly present in each commit subject or body. Do not invent behavior, scope, impact, or benefits. The commit type is a hint, but the user-visible impact decides the category. Both languages must describe exactly the same changes, one bullet for one bullet, with no additions, omissions, merges, or reclassification between languages.

Write the English section first. Use only these level-2 headings, in this order and only when non-empty:
## Features
## Improvements
## Bug Fixes

After the complete English section, output this exact separator exactly once on its own line:
${CHINESE_MARKER}

Then write the Simplified Chinese section with exactly these corresponding level-2 headings, in the same order and only when non-empty:
## 新功能
## 改进
## 错误修复

In the Chinese section, preserve links, code spans, product names, and technical identifiers.

Classify entries as follows:
- Features: newly added user-facing capabilities or workflows.
- Improvements: enhancements to existing behavior, usability, performance, reliability, or public documentation.
- Bug Fixes: incorrect behavior that was corrected.

Omit a section entirely when it has no changes. Do not create any other section or heading. Each bullet must describe a concrete user-visible change. Prefer the commit body when the subject is broad. Avoid vague statements such as "improved the workflow", "refined the update process", or "updated documentation" unless the bullet also states what changed. Do not copy a vague subject verbatim.

Merge related commits into one bullet. Omit repository-only, build-only, test-only, CI-only, and internal agent or contributor configuration changes. Do not mention commit hashes, file paths, implementation details, or tests.

If every commit is internal-only and no user-visible change remains, output exactly:
${INTERNAL_ONLY_MARKER}

${MAINTENANCE_ONLY_MESSAGES.english}

${CHINESE_MARKER}

${MAINTENANCE_ONLY_MESSAGES.chinese}

Output only the two Markdown language sections and the exact separator. Do not add a title, preamble, code fence, English-language marker, installation instructions, acknowledgments, or any other text.

# Release context

- Release: ${tag}
- Previous stable release: ${previousTag || 'none'}
- Commit range: ${range}

# Commits

${commits}`;
  return deepSeekText(prompt, apiKey, fetchImpl, 'bilingual changelog');
}

/** Generate both changelog languages, then append fixed installation notes and credits. */
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
  const bilingual = await generateBilingualChangelog({
    tag,
    previousTag,
    apiKey,
    fetchImpl,
    runGit: git,
  });
  const [english, chinese] = splitBilingualNotes(bilingual);
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
