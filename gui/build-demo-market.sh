#!/usr/bin/env bash
# Build the demo marketplace: package open-source skills (from anthropics/skills)
# into a Codex marketplace layout, so it can be added via `codex plugin marketplace add`.
set -euo pipefail

MK=/home/brewswang/marketplaces/skillhub-demo
SRC=/tmp/anthropic-skills/skills

rm -rf "$MK"
mkdir -p "$MK/.agents/plugins"
mkdir -p "$MK/plugins/office-docs/.codex-plugin" "$MK/plugins/office-docs/skills"
mkdir -p "$MK/plugins/web-toolkit/.codex-plugin" "$MK/plugins/web-toolkit/skills"

cp -r "$SRC/docx" "$SRC/pdf" "$SRC/xlsx" "$MK/plugins/office-docs/skills/"
cp -r "$SRC/webapp-testing" "$SRC/web-artifacts-builder" "$MK/plugins/web-toolkit/skills/"

cat > "$MK/.agents/plugins/marketplace.json" << 'JSON'
{
  "name": "skillhub-demo",
  "interface": {
    "displayName": "技能社区演示市场"
  },
  "plugins": [
    {
      "name": "office-docs",
      "source": {
        "source": "local",
        "path": "./plugins/office-docs"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    },
    {
      "name": "web-toolkit",
      "source": {
        "source": "local",
        "path": "./plugins/web-toolkit"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Developer Tools"
    }
  ]
}
JSON

cat > "$MK/plugins/office-docs/.codex-plugin/plugin.json" << 'JSON'
{
  "name": "office-docs",
  "version": "0.1.0",
  "description": "Office document skills: create, read, and edit Word, PDF, and Excel files. Packaged from the open-source Anthropic skills repository.",
  "homepage": "https://github.com/anthropics/skills",
  "repository": "https://github.com/anthropics/skills",
  "keywords": ["docx", "pdf", "xlsx", "office"],
  "skills": "./skills/",
  "interface": {
    "displayName": "办公文档助手",
    "shortDescription": "处理 Word / PDF / Excel：创建、读取、编辑与转换文档",
    "longDescription": "把官方技能仓库中的 docx、pdf、xlsx 三个技能打包为插件：支持 Word 文档创建与编辑、PDF 读取合并拆分、Excel 表格读写。"
  }
}
JSON

cat > "$MK/plugins/web-toolkit/.codex-plugin/plugin.json" << 'JSON'
{
  "name": "web-toolkit",
  "version": "0.1.0",
  "description": "Web development skills: automated webapp testing and web artifact building. Packaged from the open-source Anthropic skills repository.",
  "homepage": "https://github.com/anthropics/skills",
  "repository": "https://github.com/anthropics/skills",
  "keywords": ["webapp", "testing", "artifacts"],
  "skills": "./skills/",
  "interface": {
    "displayName": "Web 开发工具箱",
    "shortDescription": "网页应用自动化测试与 Web 构件生成技能",
    "longDescription": "把官方技能仓库中的 webapp-testing、web-artifacts-builder 两个技能打包为插件：支持 Playwright 驱动的网页测试与可交付 Web 构件生成。"
  }
}
JSON

echo "=== 市场目录结构 ==="
find "$MK" -maxdepth 4 -not -path "*/skills/*/*" -not -name "*.py" | sort
echo
echo "=== 技能确认 ==="
ls "$MK/plugins/office-docs/skills/"
ls "$MK/plugins/web-toolkit/skills/"
echo
echo "=== 市场总体积 ==="
du -sh "$MK"
