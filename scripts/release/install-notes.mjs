/** Fixed installation instructions, appended after generating and translating the changelog. */
export function installationNotes(language) {
  if (language === 'en') {
    return `### Installation

- **macOS**: Use \`aidget-darwin-arm64.dmg\` for Apple Silicon or \`aidget-darwin-amd64.dmg\` for Intel.
- **Windows**: Use \`aidget-windows-amd64.exe\` for x64 or \`aidget-windows-arm64.exe\` for ARM64.
- **Linux**: Use the \`aidget-linux-amd64.*\` packages on x86_64 or \`aidget-linux-arm64.*\` on ARM64. Replace \`<arch>\` below with \`amd64\` or \`arm64\` to match the downloaded file.
- **Debian/Ubuntu**: \`sudo apt install ./aidget-linux-<arch>.deb\`
- **Fedora/RHEL**: \`sudo dnf install ./aidget-linux-<arch>.rpm\`
- **openSUSE**: \`sudo zypper install ./aidget-linux-<arch>.rpm\`

> ⚠️ These packages are not signed yet. Please follow the instructions below to manually bypass your operating system's security prompts for now.

- **macOS**: Open the DMG and drag the aidget icon into Applications. Launch the app. If a security prompt appears, go to **System Settings → Privacy & Security → Open Anyway**. If macOS reports that the app is damaged or cannot be opened, use this command to clear its quarantine attribute:
  \`xattr -dr com.apple.quarantine /Applications/aidget.app\`
- **Windows**: In the security prompt, choose **More info → Run anyway**.`;
  }
  if (language === 'zh') {
    return `### 安装说明

- **macOS 系统**：Apple Silicon 芯片使用 \`aidget-darwin-arm64.dmg\`，Intel 芯片使用 \`aidget-darwin-amd64.dmg\`
- **Windows 系统**：x64 使用 \`aidget-windows-amd64.exe\`，ARM64 使用 \`aidget-windows-arm64.exe\`
- **Linux 系统**：x86_64 使用 \`aidget-linux-amd64.*\`，ARM64 使用 \`aidget-linux-arm64.*\`。下方 \`<arch>\` 请替换为 \`amd64\` 或 \`arm64\`，与下载的文件保持一致。
- **Debian/Ubuntu**：\`sudo apt install ./aidget-linux-<arch>.deb\`
- **Fedora/RHEL**：\`sudo dnf install ./aidget-linux-<arch>.rpm\`
- **openSUSE**：\`sudo zypper install ./aidget-linux-<arch>.rpm\`

> ⚠️ 由于安装包尚未进行签名，请暂时按照以下说明手动跳过操作系统的安全弹窗。

- **macOS**：打开 dmg，将 aidget 图标拖入“应用程序”。启动应用，如果弹出提示信息，前往 **系统设置 → 隐私与安全性 → 仍要打开**。如果提示已损坏或无法打开，使用此命令清除隔离属性：
  \`xattr -dr com.apple.quarantine /Applications/aidget.app\`
- **Windows**：在弹窗提示中选择 **更多信息 → 仍要运行**。`;
  }
  throw new Error(`unsupported release notes language ${language}`);
}
