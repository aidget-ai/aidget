import { describe, expect, it } from 'vite-plus/test';

import { formatAgentConfigurationFailure, systemLocale, translate } from '../../src/main/locale';

describe('main window locale', () => {
  it('follows simplified Chinese when any system language is Chinese', () => {
    expect(systemLocale(['en-US', 'zh-Hans-CN'])).toBe('zh');
    expect(systemLocale(['en-US'])).toBe('en');
    expect(translate('en', 'system')).toBe('System');
    expect(translate('zh', 'system')).toBe('跟随系统');
  });

  it('translates widget filters in both supported languages', () => {
    expect(translate('en', 'filterAll')).toBe('All');
    expect(translate('zh', 'filterEnabled')).toBe('已打开');
    expect(translate('zh', 'filterDisabled')).toBe('已关闭');
    expect(translate('zh', 'recycleBin')).toBe('回收站');
    expect(translate('zh', 'sortNewestDescription')).toBe('按创建时间倒序排列');
  });

  it('translates GitHub token network settings in both supported languages', () => {
    expect(translate('zh', 'network')).toBe('网络');
    expect(translate('zh', 'githubToken')).toBe('GitHub 令牌');
    expect(translate('zh', 'githubTokenDescription')).toBe('用于社区组件的拉取和安装。');
    expect(translate('zh', 'githubTokenPlaceholder')).toBe('ghp_xxx');
    expect(translate('zh', 'communityGithubTokenHint')).toContain('配置 GitHub 令牌');
    expect(translate('en', 'network')).toBe('Network');
    expect(translate('en', 'githubToken')).toBe('GitHub token');
  });

  it('uses compact community action labels', () => {
    expect(translate('zh', 'communityRefreshing')).toBe('刷新中');
    expect(translate('zh', 'communityInstalling')).toBe('安装中');
    expect(translate('zh', 'communityIdConflict')).toBe('已存在');
    expect(translate('zh', 'communityUnsupported')).toBe('不兼容');
    expect(translate('en', 'communityRefreshing')).toBe('Refreshing');
    expect(translate('en', 'communityInstalling')).toBe('Installing');
    expect(translate('en', 'communityIdConflict')).toBe('Already exists');
    expect(translate('en', 'communityUnsupported')).toBe('Incompatible');
  });

  it('labels integrations, local widgets, and navigation groups', () => {
    expect(translate('zh', 'integrations')).toBe('接入');
    expect(translate('zh', 'mcpAndSkills')).toBe('MCP 与技能');
    expect(translate('zh', 'mcpStepTitle')).toBe('第一步：接入 MCP 服务');
    expect(translate('zh', 'skillsStepTitle')).toBe('第二步：接入 Agent Skill');
    expect(translate('zh', 'chatStepTitle')).toBe('第三步：在 Agent 中对话');
    expect(translate('zh', 'chatStepDescription')).toBe(
      '启动已接入的 Agent，输入 /aidget 指令开始对话。',
    );
    expect(translate('zh', 'widgetsGroup')).toBe('小组件');
    expect(translate('zh', 'localWidgets')).toBe('本地组件');
    expect(translate('zh', 'communityWidgets')).toBe('社区组件');
    expect(translate('zh', 'communityNoNetwork')).toBe('无网络权限');
    expect(translate('en', 'widgetsGroup')).toBe('Widgets');
    expect(translate('zh', 'agentAll')).toBe('全部');
    expect(translate('zh', 'integrationNoAgents')).toBe('没有检测到可接入此资源的 Agent。');
  });

  it('labels appearance, behavior, and tray click options', () => {
    expect(translate('zh', 'appearance')).toBe('外观');
    expect(translate('zh', 'behavior')).toBe('行为');
    expect(translate('zh', 'trayClickAction')).toBe('点击托盘图标');
    expect(translate('zh', 'trayMenu')).toBe('打开快捷菜单');
    expect(translate('zh', 'trayWindow')).toBe('打开主窗口');
    expect(translate('en', 'appearance')).toBe('Appearance');
    expect(translate('en', 'behavior')).toBe('Behavior');
    expect(translate('en', 'trayClickAction')).toBe('Click tray icon');
    expect(translate('en', 'trayMenu')).toBe('Open quick menu');
    expect(translate('en', 'trayWindow')).toBe('Open main window');
  });

  it('describes failed Agent configuration by the number affected', () => {
    expect(formatAgentConfigurationFailure('zh', ['Codex'], ['config.toml is invalid'])).toBe(
      'Codex 配置失败：config.toml is invalid',
    );
    expect(
      formatAgentConfigurationFailure(
        'zh',
        ['Codex', 'Zed'],
        ['Codex: config.toml is invalid', 'Zed: settings.json is invalid'],
      ),
    ).toBe('多个 Agent 配置失败：Codex: config.toml is invalid；Zed: settings.json is invalid');
    expect(formatAgentConfigurationFailure('en', ['Codex'], ['config.toml is invalid'])).toBe(
      'Could not configure Codex: config.toml is invalid',
    );
  });
});
