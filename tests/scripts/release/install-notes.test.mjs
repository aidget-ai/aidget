import { expect, test } from 'vite-plus/test';

import { installationNotes } from '../../../scripts/release/install-notes.mjs';

test('both languages provide package choices and security prompt instructions', () => {
  for (const lang of ['en', 'zh']) {
    const text = installationNotes(lang);
    expect(text).toContain('`aidget-darwin-arm64.dmg`');
    expect(text).toContain('`aidget-darwin-amd64.dmg`');
    expect(text).toContain('`aidget-windows-amd64.exe`');
    expect(text).toContain('> ⚠️');
    expect(text).not.toContain('xattr');
    expect(text).not.toContain('Linux');
  }
  expect(installationNotes('en')).toContain('**More info → Run anyway**');
  expect(installationNotes('zh')).toBe(`### 安装说明

- macOS 系统：Apple Silicon 芯片使用 \`aidget-darwin-arm64.dmg\`，Intel 芯片使用 \`aidget-darwin-amd64.dmg\`
- Windows 系统：通常使用 \`aidget-windows-amd64.exe\`

> ⚠️ 由于安装包尚未进行签名，请暂时按照以下说明手动跳过操作系统的安全弹窗。

- **macOS：**打开 dmg，将 aidget 图标拖入“应用程序”。启动应用，如果弹出提示信息，前往 **系统设置 → 隐私与安全性 → 仍要打开**。
- **Windows：**在弹窗提示中选择 **更多信息 → 仍要运行**。`);
});
