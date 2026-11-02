import type { CommunityEntry } from '../../src/types/rust';

/** A reviewed entry with distinct immutable media for scheduling tests. */
export function communityEntry(index = 0): CommunityEntry {
  return {
    entry: {
      publishedAt: '2026-10-31T00:00:00Z',
      package: {
        name: `@aidget-community/widget-${index}`,
        version: '1.0.0',
        description: 'Example',
        author: 'Community',
        repository: null,
        license: 'MIT',
        files: [],
      },
      config: {
        id: `widget-${index}`,
        title: `Widget ${index}`,
        logo: null,
        screenshots: ['screen.png'],
        platforms: ['macos', 'windows', 'linux'],
        size: { width: 360, height: 220 },
        dragMode: 'all',
        permissions: [],
        network: [],
      },
      files: [{ path: 'screen.png', size: 32, sha256: index.toString(16).padStart(64, '0') }],
    },
    installed: false,
    idConflict: false,
    supported: true,
  };
}
