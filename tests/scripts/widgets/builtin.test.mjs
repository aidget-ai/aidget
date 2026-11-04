import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, sep } from 'node:path';

import { test } from 'vite-plus/test';
const registry = JSON.parse(
  readFileSync(new URL('../../../widgets/registry.json', import.meta.url), 'utf8'),
);
test('bundled packages match the reviewed community file snapshot', () => {
  const bundled = readdirSync('widgets', { withFileTypes: true }).filter((dir) =>
    dir.isDirectory(),
  );
  assert.ok(bundled.length > 0, 'no bundled packages');
  for (const directory of bundled) {
    const entry = registry.widgets.find((item) => item.config.id === directory.name);
    assert.ok(entry, `${directory.name} is missing from the reviewed registry`);
    const root = join('widgets', directory.name);
    const paths = [];
    for (const file of readdirSync(root, { recursive: true, withFileTypes: true })) {
      assert.ok(
        file.isDirectory() || file.isFile(),
        `${file.name} must be a regular file or directory`,
      );
      if (file.isFile()) {
        paths.push(relative(root, join(file.parentPath, file.name)).split(sep).join('/'));
      }
    }
    assert.deepEqual(
      paths.sort((left, right) => left.localeCompare(right)),
      entry.files.map((file) => file.path).sort((left, right) => left.localeCompare(right)),
      directory.name,
    );
    for (const file of entry.files) {
      const bytes = readFileSync(join(root, file.path));
      assert.equal(bytes.length, file.size, file.path);
      assert.equal(createHash('sha256').update(bytes).digest('hex'), file.sha256, file.path);
    }
  }
});
