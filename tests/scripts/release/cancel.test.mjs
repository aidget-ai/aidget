import { describe, expect, test, vi } from 'vite-plus/test';

import { cancelOlderReleases } from '../../../scripts/release/cancel.mjs';

function run(id, tag, status = 'in_progress') {
  return { id, head_branch: tag, status, event: 'push' };
}

function response(value, status = 200) {
  return new Response(status === 202 ? null : JSON.stringify(value), { status });
}

describe('cancel older releases', () => {
  test('only cancels active lower versions, including manual betas', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(
        response({
          workflow_runs: [
            run(1, 'v0.1.3'),
            run(2, 'v0.1.4-beta.2'),
            run(3, 'v0.1.4'),
            run(4, 'v0.1.5'),
            run(5, 'v0.1.2', 'completed'),
            run(6, 'main'),
          ],
        }),
      )
      .mockResolvedValue(response(null, 202));
    await cancelOlderReleases({
      tag: 'v0.1.4',
      repository: 'aidget-ai/aidget',
      token: 'test',
      fetch,
    });
    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      'https://api.github.com/repos/aidget-ai/aidget/actions/workflows/release.yml/runs?event=push&per_page=100&page=1',
      'https://api.github.com/repos/aidget-ai/aidget/actions/runs/1/cancel',
      'https://api.github.com/repos/aidget-ai/aidget/actions/runs/2/cancel',
    ]);
    expect(fetch.mock.calls[1][1]).toMatchObject({ method: 'POST' });
  });

  test('paginates before cancellation so changing statuses cannot skip a page', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(
        response({
          workflow_runs: Array.from({ length: 100 }, (_, i) => run(i, 'v0.1.1', 'completed')),
        }),
      )
      .mockResolvedValueOnce(response({ workflow_runs: [run(101, 'v0.1.2', 'queued')] }))
      .mockResolvedValueOnce(response(null, 202));
    await cancelOlderReleases({
      tag: 'v0.1.4',
      repository: 'aidget-ai/aidget',
      token: 'test',
      fetch,
    });
    expect(fetch.mock.calls[1][0]).toContain('page=2');
    expect(fetch.mock.calls[2][0]).toContain('/101/cancel');
  });

  test('accepts a cancellation conflict only when the old run has already completed', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValueOnce(response({ workflow_runs: [run(1, 'v0.1.3')] }))
      .mockResolvedValueOnce(response({}, 409))
      .mockResolvedValueOnce(response({ status: 'completed' }));
    await expect(
      cancelOlderReleases({ tag: 'v0.1.4', repository: 'aidget-ai/aidget', token: 'test', fetch }),
    ).resolves.toBe(0);
    expect(fetch.mock.calls[2][0]).toBe(
      'https://api.github.com/repos/aidget-ai/aidget/actions/runs/1',
    );
  });

  test('API errors fail explicitly without exposing credentials', async () => {
    const fetch = vi.fn().mockResolvedValue(response({}, 403));
    await expect(
      cancelOlderReleases({
        tag: 'v0.1.4',
        repository: 'aidget-ai/aidget',
        token: 'sensitive',
        fetch,
      }),
    ).rejects.toThrow('GitHub API returned 403');
  });

  test('a late old run never cancels a newer or equal version', async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        response({ workflow_runs: [run(1, 'v0.1.4'), run(2, 'v0.1.5-beta.0', 'queued')] }),
      );
    await expect(
      cancelOlderReleases({ tag: 'v0.1.4', repository: 'aidget-ai/aidget', token: 'test', fetch }),
    ).resolves.toBe(0);
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  test('an active conflict and cancellation errors abort the new release', async () => {
    for (const status of [403, 409, 500]) {
      const fetch = vi
        .fn()
        .mockResolvedValueOnce(response({ workflow_runs: [run(1, 'v0.1.3')] }))
        .mockResolvedValueOnce(response({}, status))
        .mockResolvedValueOnce(response({ status: 'in_progress' }));
      await expect(
        cancelOlderReleases({
          tag: 'v0.1.4',
          repository: 'aidget-ai/aidget',
          token: 'test',
          fetch,
        }),
      ).rejects.toThrow(`GitHub API returned ${status}`);
    }
  });

  test('validates API responses and credentials before attempting cancellation', async () => {
    const fetch = vi.fn().mockResolvedValue(response({}));
    await expect(
      cancelOlderReleases({ tag: 'v0.1.4', repository: 'aidget-ai/aidget', token: 'test', fetch }),
    ).rejects.toThrow('invalid release runs');
    fetch.mockClear();
    await expect(
      cancelOlderReleases({ tag: 'v0.1.4', repository: 'aidget-ai/aidget', token: '', fetch }),
    ).rejects.toThrow('Actions token');
    expect(fetch).not.toHaveBeenCalled();
  });
});
