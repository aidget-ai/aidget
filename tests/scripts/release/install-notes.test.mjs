import { expect, test } from 'vite-plus/test';

import { installationNotes } from '../../../scripts/release/install-notes.mjs';

test('both languages provide package choices and security prompt instructions', () => {
  for (const lang of ['en', 'zh']) {
    const text = installationNotes(lang);
    expect(text).toContain('`aidget-darwin-arm64.dmg`');
    expect(text).toContain('`aidget-darwin-amd64.dmg`');
    expect(text).toContain('`aidget-windows-amd64.exe`');
    expect(text).toContain('> ⚠️');
    expect(text).toContain('xattr -dr com.apple.quarantine /Applications/aidget.app');
    expect(text).not.toContain('Linux');
  }
  expect(installationNotes('en')).toBe(`### Installation

> Expand **Assets** below to download an installer.

- **macOS**: Use \`aidget-darwin-arm64.dmg\` for Apple Silicon or \`aidget-darwin-amd64.dmg\` for Intel.
- **Windows**: Usually, use \`aidget-windows-amd64.exe\`.

> ⚠️ These packages are not signed yet. Please follow the instructions below to manually bypass your operating system's security prompts for now.

- **macOS**: Open the DMG and drag the aidget icon into Applications. Launch the app. If a security prompt appears, go to **System Settings → Privacy & Security → Open Anyway**. If macOS reports that the app is damaged or cannot be opened, use this command to clear its quarantine attribute:
  \`xattr -dr com.apple.quarantine /Applications/aidget.app\`
- **Windows**: In the security prompt, choose **More info → Run anyway**.`);
  expect(installationNotes('zh')).toBe(`### 安装说明

> 展开下方 Assets 下载安装包。

- **macOS 系统**：Apple Silicon 芯片使用 \`aidget-darwin-arm64.dmg\`，Intel 芯片使用 \`aidget-darwin-amd64.dmg\`
- **Windows 系统**：通常使用 \`aidget-windows-amd64.exe\`

> ⚠️ 由于安装包尚未进行签名，请暂时按照以下说明手动跳过操作系统的安全弹窗。

- **macOS**：打开 dmg，将 aidget 图标拖入“应用程序”。启动应用，如果弹出提示信息，前往 **系统设置 → 隐私与安全性 → 仍要打开**。如果提示已损坏或无法打开，使用此命令清除隔离属性：
  \`xattr -dr com.apple.quarantine /Applications/aidget.app\`
- **Windows**：在弹窗提示中选择 **更多信息 → 仍要运行**。`);
});
