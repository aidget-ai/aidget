import { describe, expect, it } from 'vite-plus/test';

import {
  formatAgentConfigurationFailure,
  matchSystemLanguage,
  translate,
} from '../../src/main/locale';

describe('main window locale', () => {
  it('keeps the shared theme system label in both locales', () => {
    expect(translate('en-US', 'system')).toBe('System');
    expect(translate('zh-CN', 'system')).toBe('跟随系统');
  });

  it('translates widget filters in both supported languages', () => {
    expect(translate('en-US', 'filterAll')).toBe('All');
    expect(translate('zh-CN', 'filterEnabled')).toBe('已打开');
    expect(translate('zh-CN', 'filterDisabled')).toBe('已关闭');
    expect(translate('zh-CN', 'recycleBin')).toBe('回收站');
    expect(translate('zh-CN', 'sortNewestDescription')).toBe('按创建时间倒序排列');
  });

  it('translates GitHub token network settings in both supported languages', () => {
    expect(translate('zh-CN', 'network')).toBe('网络');
    expect(translate('zh-CN', 'githubToken')).toBe('GitHub 令牌');
    expect(translate('zh-CN', 'githubTokenDescription')).toBe(
      '用于社区组件的拉取和安装，以及更新检查。',
    );
    expect(translate('zh-CN', 'githubTokenPlaceholder')).toBe('ghp_xxx');
    expect(translate('zh-CN', 'communityGithubTokenHint')).toContain('配置 GitHub 令牌');
    expect(translate('en-US', 'network')).toBe('Network');
    expect(translate('en-US', 'githubToken')).toBe('GitHub token');
    expect(translate('en-US', 'githubTokenDescription')).toBe(
      'Used to load and install community widgets and check for updates.',
    );
  });

  it('uses compact community action labels', () => {
    expect(translate('zh-CN', 'communityRefreshing')).toBe('刷新中');
    expect(translate('zh-CN', 'communityInstalling')).toBe('安装中');
    expect(translate('zh-CN', 'communityInstalledNotice')).toBe('已安装到本地组件');
    expect(translate('zh-CN', 'communityIdConflict')).toBe('已存在');
    expect(translate('zh-CN', 'communityUnsupported')).toBe('不兼容');
    expect(translate('en-US', 'communityRefreshing')).toBe('Refreshing');
    expect(translate('en-US', 'communityInstalling')).toBe('Installing');
    expect(translate('en-US', 'communityIdConflict')).toBe('Already exists');
    expect(translate('en-US', 'communityUnsupported')).toBe('Incompatible');
  });

  it('labels integrations, local widgets, and navigation groups', () => {
    expect(translate('zh-CN', 'integrations')).toBe('接入');
    expect(translate('zh-CN', 'mcpAndSkills')).toBe('MCP 与技能');
    expect(translate('zh-CN', 'mcpStepTitle')).toBe('第一步：接入 MCP 服务');
    expect(translate('zh-CN', 'skillsStepTitle')).toBe('第二步：接入 Agent Skill');
    expect(translate('zh-CN', 'chatStepTitle')).toBe('第三步：在 Agent 中对话');
    expect(translate('zh-CN', 'chatStepDescription')).toBe(
      '启动已接入的 Agent，输入 /aidget 指令开始对话。',
    );
    expect(translate('zh-CN', 'widgetsGroup')).toBe('小组件');
    expect(translate('zh-CN', 'localWidgets')).toBe('本地组件');
    expect(translate('zh-CN', 'communityWidgets')).toBe('社区组件');
    expect(translate('zh-CN', 'communityNoNetwork')).toBe('无网络权限');
    expect(translate('en-US', 'widgetsGroup')).toBe('Widgets');
    expect(translate('zh-CN', 'agentAll')).toBe('全部');
    expect(translate('zh-CN', 'integrationNoAgents')).toBe('没有检测到可接入此资源的 Agent。');
    expect(translate('zh-CN', 'installFor')).toBe('为 {agent} 安装：');
  });

  it('labels appearance, behavior, and tray click options', () => {
    expect(translate('zh-CN', 'appearance')).toBe('外观');
    expect(translate('zh-CN', 'behavior')).toBe('行为');
    expect(translate('zh-CN', 'trayClickAction')).toBe('点击托盘图标');
    expect(translate('zh-CN', 'trayMenu')).toBe('打开快捷菜单');
    expect(translate('zh-CN', 'trayWindow')).toBe('打开主窗口');
    expect(translate('en-US', 'appearance')).toBe('Appearance');
    expect(translate('en-US', 'behavior')).toBe('Behavior');
    expect(translate('en-US', 'trayClickAction')).toBe('Click tray icon');
    expect(translate('en-US', 'trayMenu')).toBe('Open quick menu');
    expect(translate('en-US', 'trayWindow')).toBe('Open main window');
  });

  it('describes failed Agent configuration by the number affected', () => {
    expect(formatAgentConfigurationFailure('zh-CN', ['Codex'], ['config.toml is invalid'])).toBe(
      'Codex 配置失败：config.toml is invalid',
    );
    expect(
      formatAgentConfigurationFailure(
        'zh-CN',
        ['Codex', 'Zed'],
        ['Codex: config.toml is invalid', 'Zed: settings.json is invalid'],
      ),
    ).toBe('多个 Agent 配置失败：Codex: config.toml is invalid；Zed: settings.json is invalid');
    expect(formatAgentConfigurationFailure('en-US', ['Codex'], ['config.toml is invalid'])).toBe(
      'Could not configure Codex: config.toml is invalid',
    );
  });

  it('matches the first supported system language in preference order', () => {
    expect(matchSystemLanguage(['en-US', 'zh-Hans-CN'])).toBe('en-US');
    expect(matchSystemLanguage(['zh-TW', 'en-US'])).toBe('zh-CN');
    expect(matchSystemLanguage(['ja-JP', 'en-GB', 'zh-CN'])).toBe('en-US');
    expect(matchSystemLanguage(['ja-JP', 'zh-HK'])).toBe('zh-CN');
    expect(matchSystemLanguage(['ZH-hans-cn'])).toBe('zh-CN');
    expect(matchSystemLanguage(['en-Latn-GB'])).toBe('en-US');
    expect(matchSystemLanguage(['ja-JP', 'fr-FR'])).toBe('en-US');
    expect(matchSystemLanguage([])).toBe('en-US');
  });

  it('matches only BCP 47 language subtags', () => {
    expect(matchSystemLanguage(['zhfoo', 'en-US'])).toBe('en-US');
    expect(matchSystemLanguage(['english', 'zh-Hant'])).toBe('zh-CN');
    expect(matchSystemLanguage(['_zh', 'en_US'])).toBe('en-US');
  });
});
