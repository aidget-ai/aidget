/** Fixed installation instructions, appended after generating and translating the changelog. */
export function installationNotes(language) {
  if (language === 'en') {
    return `### Installation

- macOS: Use \`aidget-darwin-arm64.dmg\` for Apple Silicon or \`aidget-darwin-amd64.dmg\` for Intel.
- Windows: Usually, use \`aidget-windows-amd64.exe\`.

> ⚠️ These packages are not signed yet. Please follow the instructions below to manually bypass your operating system's security prompts for now.

- **macOS:** Open the DMG and drag the aidget icon into Applications. Launch the app. If a security prompt appears, go to **System Settings → Privacy & Security → Open Anyway**.
- **Windows:** In the security prompt, choose **More info → Run anyway**.`;
  }
  if (language === 'zh') {
    return `### 安装说明

- macOS 系统：Apple Silicon 芯片使用 \`aidget-darwin-arm64.dmg\`，Intel 芯片使用 \`aidget-darwin-amd64.dmg\`
- Windows 系统：通常使用 \`aidget-windows-amd64.exe\`

> ⚠️ 由于安装包尚未进行签名，请暂时按照以下说明手动跳过操作系统的安全弹窗。

- **macOS：**打开 dmg，将 aidget 图标拖入“应用程序”。启动应用，如果弹出提示信息，前往 **系统设置 → 隐私与安全性 → 仍要打开**。
- **Windows：**在弹窗提示中选择 **更多信息 → 仍要运行**。`;
  }
  throw new Error(`unsupported release notes language ${language}`);
}
