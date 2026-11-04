import { describe, expect, test } from 'vite-plus/test';

import {
  addApprovedContributor,
  canApprove,
  formatApprovalComment,
  parseApprovalCommand,
  parseApprovedContributors,
  UNAPPROVED_PR_MESSAGE,
} from '../../../scripts/contributors/approval.mjs';

describe('contributor approval', () => {
  test('accepts lgtmi only as the command at the start of a comment', () => {
    for (const body of [
      'lgtmi',
      'lgtmi.',
      'LGTMI approved',
      'lgtmi: looks good',
      'lgtmi\nreason',
    ]) {
      expect(parseApprovalCommand(body)).toBe(true);
    }

    for (const body of ['', 'lgtm', 'not lgtmi', 'please lgtmi', 'lgtmi-approved']) {
      expect(parseApprovalCommand(body)).toBe(false);
    }
  });

  test('only admins can approve contributors', () => {
    expect(canApprove('admin')).toBe(true);

    for (const permission of ['maintain', 'write', 'read', 'triage', 'none', undefined]) {
      expect(canApprove(permission)).toBe(false);
    }
  });

  test('allowlist parsing ignores comments and compares logins case-insensitively', () => {
    const contributors = parseApprovedContributors(`
# Maintainer-approved pull request authors
octocat
Hubot
`);

    expect(contributors).toEqual(new Set(['octocat', 'hubot']));
  });

  test('adding a contributor preserves the file and is idempotent', () => {
    const initial = '# Approved pull request authors\n\noctocat\n';

    const first = addApprovedContributor(initial, 'Hubot');
    expect(first.status).toBe('changed');
    expect(first.content).toBe(`${initial}Hubot\n`);

    const second = addApprovedContributor(first.content, 'hubot');
    expect(second.status).toBe('already');
    expect(second.content).toBe(first.content);
  });

  test('approval comments use the contributor-facing confirmation', () => {
    expect(formatApprovalComment('changed', 'octocat')).toBe(
      '@octocat, you are approved to open pull requests.',
    );
    expect(formatApprovalComment('already', 'octocat')).toBeNull();
  });

  test('unapproved pull requests receive the documented message', () => {
    expect(UNAPPROVED_PR_MESSAGE).toBe(
      [
        'This pull request was closed because you are not approved to contribute code.',
        '',
        'Open a Contribution Proposal Issue and discuss the change with maintainers.',
        'Admins can reply `lgtmi` to approve for future pull requests.',
      ].join('\n'),
    );
  });
});
