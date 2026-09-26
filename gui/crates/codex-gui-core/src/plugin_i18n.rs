//! Simplified-Chinese display copy for the curated plugin marketplaces.
//!
//! The curated catalogs ship English display names and one-line
//! descriptions. The market tab renders the Chinese copy below for the
//! curated inventory and passes the wire metadata through for anything the
//! table does not know, so plugins published after this table ships still
//! render.

/// Curated plugins with Chinese display copy: `(wire name, display name,
/// one-line description)`.
const CHINESE_CATALOG: &[(&str, &str, &str)] = &[
    ("adobe", "Adobe", "设计、合成与编辑"),
    ("airtable", "Airtable", "为 ChatGPT 添加结构化数据"),
    (
        "atlassian-rovo",
        "Atlassian Rovo",
        "管理 Jira 和 Confluence",
    ),
    (
        "boltz-api-cli",
        "Boltz",
        "预测结构、筛选分子与蛋白质并设计结合物",
    ),
    (
        "build-ios-apps",
        "构建 iOS 应用",
        "使用 App Intents、SwiftUI 和 Xcode 工作流构建、打磨与调试 iOS 应用",
    ),
    (
        "build-macos-apps",
        "构建 macOS 应用",
        "借助 SwiftUI 与 AppKit 指导构建、调试、分析与实现 macOS 应用",
    ),
    (
        "build-web-apps",
        "构建 Web 应用",
        "构建前端 Web 应用：素材生成、浏览器测试、支付与数据库",
    ),
    (
        "build-web-data-visualization",
        "Web 数据可视化",
        "设计、构建、测试并导出 Web 数据可视化",
    ),
    ("canva", "Canva", "创建、评审和编辑设计"),
    ("chatcut", "ChatCut", "安装并连接 ChatCut"),
    ("circleci", "CircleCI", "构建、测试并部署任意应用"),
    ("clickup", "ClickUp", "把 Codex 变成你的 ClickUp 指挥中心"),
    (
        "cloudflare",
        "Cloudflare",
        "结合官方 MCP 的 Cloudflare 平台指导",
    ),
    (
        "coderabbit",
        "CodeRabbit",
        "对当前改动运行 AI 驱动的代码评审",
    ),
    ("codex-security", "Codex 安全扫描", "面向代码库的安全扫描"),
    ("consensus", "Consensus", "探索科学研究"),
    (
        "creative-production",
        "创意制作",
        "根据简报或产品图片制作营销视觉素材",
    ),
    ("data-analytics", "数据分析", "用数据回答产品和业务问题"),
    ("datadog", "Datadog（预览）", "搜索并操作你的数据"),
    ("dropbox", "Dropbox", "访问、保存和分享文件"),
    (
        "expo",
        "Expo",
        "构建、部署、升级和调试 Expo 与 React Native 应用",
    ),
    ("figma", "Figma", "Figma 设计到代码工作流"),
    ("game-studio", "游戏工作室", "设计、原型化并发布浏览器游戏"),
    ("github", "GitHub", "处理 PR、issue、CI 与发布流程"),
    ("gmail", "Gmail", "阅读和管理 Gmail 邮件"),
    ("google-calendar", "Google 日历", "管理 Google 日历事件"),
    (
        "google-drive",
        "Google Drive",
        "处理 Drive、文档、表格和幻灯片",
    ),
    ("granola", "Granola", "补充你的会议上下文"),
    ("higgsfield", "Higgsfield", "涵盖所有图像与视频模型"),
    (
        "hyperframes",
        "HyperFrames（HeyGen）",
        "编写 HTML，渲染视频",
    ),
    (
        "life-science-research",
        "生命科学研究",
        "通用生命科学研究：任务路由、证据综合与可选的并行子代理分析",
    ),
    ("linear", "Linear", "规划并构建产品"),
    ("lovable", "Lovable", "构建应用与网站"),
    (
        "magicpath",
        "MagicPath",
        "在 Codex 中查找、安装和编写 MagicPath UI 组件",
    ),
    (
        "mixpanel-headless",
        "Mixpanel Headless",
        "用 Python 分析 Mixpanel 数据",
    ),
    ("monday-com", "monday.com", "管理项目、任务与 CRM"),
    (
        "ngs-analysis",
        "生命科学 NGS 分析",
        "面向测序分析的 NGS 路由指导与本地执行",
    ),
    (
        "notion",
        "Notion",
        "覆盖规格、调研、会议与知识沉淀的 Notion 工作流",
    ),
    (
        "nvidia",
        "NVIDIA",
        "NVIDIA AI、GPU、机器人、仿真与 3D 工作流指导",
    ),
    (
        "openai-ads-conversions",
        "OpenAI 广告转化",
        "配置 OpenAI Ads Pixel 与 CAPI 跟踪",
    ),
    (
        "openai-developers",
        "OpenAI 开发者",
        "按 OpenAI 最佳实践开发 AI 应用、智能体与 ChatGPT 应用",
    ),
    ("outlook-calendar", "Outlook 日历", "管理 Outlook 日程"),
    ("outlook-email", "Outlook 邮箱", "分类处理 Outlook 收件箱"),
    (
        "plugin-eval",
        "插件评测",
        "从对话开始，在本地评测或跑基准测试",
    ),
    ("posthog", "PostHog", "分析你的产品数据"),
    ("product-design", "产品设计", "探索并原型化想法"),
    (
        "public-equity-investing",
        "公开股票投资",
        "公开股票 PM 研究、多空策略、财报、ETF/指数尽调与备忘录",
    ),
    ("remotion", "Remotion", "用智能体制作视频"),
    ("sentry", "Sentry", "查看最近的 Sentry issue 与事件"),
    ("sharepoint", "SharePoint", "汇总 SharePoint 内容"),
    ("shopify", "Shopify", "构建和管理你的商店"),
    ("slack", "Slack", "阅读和管理 Slack"),
    ("stripe", "Stripe", "收款并增长营收"),
    ("supabase", "Supabase", "管理和查询数据库"),
    ("superpowers", "Superpowers", "规划、开发和调试代码"),
    ("teams", "Teams", "汇总 Teams 内容并跟进"),
    ("temporal", "Temporal", "使用 Temporal 进行开发"),
    (
        "test-android-apps",
        "测试 Android 应用",
        "在 Android 模拟器上复现问题、检查 UI 并采集性能证据",
    ),
    (
        "twilio-developer-kit",
        "Twilio 开发套件",
        "用于在 Twilio 上构建、调试和发布的 Twilio Skills",
    ),
    ("vercel", "Vercel", "构建并部署 Web 应用与智能体"),
    ("zotero", "Zotero", "从 Zotero 查找论文并添加引用"),
    ("zoom", "Zoom", "来自 Zoom 的智能会议洞察"),
];

/// Localizes one plugin's display name and one-line description.
///
/// Curated plugins render the Chinese copy; plugins outside the table keep
/// their wire name and whatever description the wire provided (`None` stays
/// `None`).
pub fn localize(plugin_name: &str, description: Option<&str>) -> (String, Option<String>) {
    match CHINESE_CATALOG
        .iter()
        .find(|(name, _, _)| *name == plugin_name)
    {
        Some((_, display_name, short_description)) => (
            display_name.to_string(),
            Some(short_description.to_string()),
        ),
        None => (plugin_name.to_string(), description.map(str::to_string)),
    }
}

#[cfg(test)]
#[path = "plugin_i18n_tests.rs"]
mod tests;
