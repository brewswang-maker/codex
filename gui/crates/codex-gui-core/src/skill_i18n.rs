//! Simplified-Chinese one-line descriptions for the composer skill picker.
//!
//! The wire descriptions are English paragraphs written for the model, not
//! one-glance menu copy. The picker renders the Chinese one-liners from this
//! table for the workspace, built-in, and curated-plugin skills; anything
//! outside the table falls back to a condensed form of its wire description
//! so skills added later still render.
//!
//! Plugin skills are keyed by their `plugin:skill` wire name, exactly as
//! `skills/list` spells them.

/// Skills with Chinese picker copy: `(wire skill name, one-line
/// description)`.
const CHINESE_SKILLS: &[(&str, &str)] = &[
    ("babysit-pr", "盯守 PR 的评审意见、CI 与合并状态并诊断失败"),
    (
        "canva:canva-brand-check",
        "对照品牌规范检查设计：配色、字体、Logo 与文案",
    ),
    (
        "canva:canva-branded-presentation",
        "按大纲或简报生成符合品牌的演示文稿",
    ),
    (
        "canva:canva-bulk-create",
        "用品牌模板与表格数据批量生成设计",
    ),
    (
        "canva:canva-design-feedback",
        "对设计给出结构化、可执行的反馈",
    ),
    ("canva:canva-edit-design", "编辑既有设计的文字、图片与排版"),
    (
        "canva:canva-implement-feedback",
        "把评审评论落实到设计，标注待人工决策项",
    ),
    (
        "canva:canva-resize-for-social-media",
        "把设计批量调整为多种社交媒体尺寸",
    ),
    (
        "canva:canva-translate-design",
        "把设计内全部文字翻译为其他语言",
    ),
    ("code-breaking-changes", "检查对外集成面的破坏性变更"),
    ("code-review", "对拉取请求做收尾代码评审"),
    ("code-review-change-size", "改动规模规范：单次不超过 800 行"),
    ("code-review-context", "评审模型可见上下文的写法"),
    ("code-review-testing", "评审测试编写规范"),
    ("codex-pr-body", "更新一个或多个 PR 的标题与正文"),
    (
        "codex-security:assess-patch-risk",
        "评估补丁影响面与合并风险",
    ),
    (
        "codex-security:attack-path-analysis",
        "对安全发现做攻击路径分析",
    ),
    ("codex-security:deep-security-scan", "执行更深入的安全扫描"),
    (
        "codex-security:define-security-policy",
        "编写限定范围的 SECURITY.md 扫描指引",
    ),
    ("codex-security:finding-discovery", "主动发现安全缺陷"),
    ("codex-security:fix-finding", "修复并验证指定的安全缺陷"),
    (
        "codex-security:propose-security-hardening",
        "设计有证据支撑的安全加固方案",
    ),
    ("codex-security:security-diff-scan", "对 Git 差异做安全评审"),
    ("codex-security:security-scan", "对仓库或指定路径做安全扫描"),
    ("codex-security:threat-model", "建立代码库威胁模型"),
    (
        "codex-security:track-findings",
        "在工单系统或公告中跟踪安全发现",
    ),
    ("codex-security:triage-finding", "导入并分诊安全漏洞票据"),
    ("codex-security:validation", "验证某个安全发现的真实性"),
    (
        "codex-security:verify-fix",
        "不改代码，验证安全修复是否生效",
    ),
    (
        "codex-security:vulnerability-writeup",
        "撰写自洽且有来源支撑的漏洞报告",
    ),
    ("imagegen", "生成或编辑位图图像（照片、插画、纹理等）"),
    (
        "office-docs:docx",
        "创建、读取与编辑 Word 文档（.docx/.dotx）",
    ),
    ("office-docs:pdf", "处理 PDF：合并拆分、水印、表单与 OCR"),
    (
        "office-docs:xlsx",
        "创建、读取与编辑电子表格（.xlsx/.csv 等）",
    ),
    ("openai-docs", "查询 OpenAI 与 Codex 官方文档"),
    ("path-types", "为操作系统路径选择 Rust 类型"),
    ("plugin-creator", "脚手架生成插件与市场条目"),
    ("product-design:audit", "先截屏取证，再评审产品体验与设计"),
    ("product-design:design-qa", "对照视觉基准做原型 QA 比对"),
    ("product-design:get-context", "设计与构建前先确认任务简报"),
    (
        "product-design:ideate",
        "用 Image Gen 生成设计草稿与方案变体",
    ),
    ("product-design:image-to-code", "把选定图像还原为响应式前端"),
    (
        "product-design:index",
        "产品设计工作总入口：探索、研究、评审与分享",
    ),
    (
        "product-design:research",
        "快速研究产品的 UX 痛点与流程摩擦",
    ),
    ("product-design:share", "部署可运行原型并返回分享链接"),
    (
        "product-design:url-to-code",
        "把线上 URL 克隆为轻量可运行原型",
    ),
    (
        "product-design:user-context",
        "保存并读取产品设计用户上下文",
    ),
    ("remote-tests", "在集成测试中覆盖远程执行器场景"),
    (
        "remotion:remotion-best-practices",
        "Remotion 任务的工作流路由",
    ),
    ("remotion:remotion-captions", "转录、展示并动画化字幕"),
    ("remotion:remotion-create", "创建新的 Remotion 视频项目"),
    ("remotion:remotion-docs", "检索最新 Remotion 文档"),
    (
        "remotion:remotion-interactivity",
        "让合成在 Remotion Studio 中可交互编辑",
    ),
    ("remotion:remotion-maps", "制作动态地图动画"),
    (
        "remotion:remotion-markup",
        "应用 Remotion 动画与特效最佳实践",
    ),
    ("remotion:remotion-multimedia", "在浏览器中检查音视频媒体"),
    ("remotion:remotion-render", "导出 Remotion 视频与静态帧"),
    ("remotion:remotion-saas", "构建可预览与渲染视频的应用"),
    ("remotion:remotion-studio", "预览视频（Remotion Studio）"),
    ("remotion:remotion-upgrade", "升级 Remotion 及相关依赖"),
    ("review-agent", "在代码改动中查找可执行的缺陷"),
    ("skill-creator", "创建或更新 Codex 技能"),
    ("skill-installer", "从精选列表或其他仓库安装技能"),
    ("superpowers:brainstorming", "动手前先澄清意图、需求与设计"),
    (
        "superpowers:dispatching-parallel-agents",
        "把相互独立的任务拆分给并行子代理",
    ),
    (
        "superpowers:executing-plans",
        "按既定实施计划执行，设评审检查点",
    ),
    (
        "superpowers:finishing-a-development-branch",
        "收尾开发分支并选择合入方式",
    ),
    (
        "superpowers:receiving-code-review",
        "收到评审意见先核实再实现，不盲从",
    ),
    (
        "superpowers:requesting-code-review",
        "完成或合并前发起聚焦的代码评审",
    ),
    (
        "superpowers:subagent-driven-development",
        "用分阶段子代理评审推进实施计划",
    ),
    (
        "superpowers:systematic-debugging",
        "先系统定位根因，再提出修复",
    ),
    (
        "superpowers:test-driven-development",
        "先写测试再写实现，用于功能与修复",
    ),
    (
        "superpowers:using-git-worktrees",
        "用 git worktree 创建隔离工作区",
    ),
    (
        "superpowers:using-superpowers",
        "说明何时与如何调用 Superpowers 技能",
    ),
    (
        "superpowers:verification-before-completion",
        "宣称完成前先用命令输出验证",
    ),
    ("superpowers:writing-plans", "把规格与需求写成多步实施计划"),
    ("superpowers:writing-skills", "创建、编辑并在部署前验证技能"),
    ("test-tui", "交互式测试 Codex TUI 的指南"),
    ("update-v8-version", "升级 V8/rusty_v8 并验证发布候选"),
];

/// Longest visible fallback, in characters.
const FALLBACK_LIMIT: usize = 80;

/// Localizes one skill's picker description.
///
/// Known skills render the Chinese one-liner; anything else keeps a
/// condensed form of the wire description (`""` stays `""`).
pub fn localize(name: &str, description: &str) -> String {
    match CHINESE_SKILLS.iter().find(|(skill, _)| *skill == name) {
        Some((_, chinese)) => chinese.to_string(),
        None => condense(description),
    }
}

/// Collapses one wire description into a single short line: whitespace is
/// squeezed, a Chinese full stop ends the excerpt, and overlong text is cut
/// at [`FALLBACK_LIMIT`] characters with an ellipsis.
fn condense(description: &str) -> String {
    let flat = description.split_whitespace().collect::<Vec<_>>().join(" ");
    let sentence = flat
        .char_indices()
        .find(|(_, ch)| matches!(ch, '。' | '！' | '？'))
        .map_or(flat.as_str(), |(index, ch)| &flat[..index + ch.len_utf8()]);
    if sentence.chars().count() <= FALLBACK_LIMIT {
        return sentence.to_string();
    }
    let mut truncated: String = sentence.chars().take(FALLBACK_LIMIT).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
#[path = "skill_i18n_tests.rs"]
mod tests;
