---
name: hypit-rs
description: Drive the hypit-rs short-video engine (SVML script to rendered MP4) from an agent. Use when the user asks to build, preview, clone, transcribe, restyle or batch-render a short video, edit a `.svml` program, wire BYOK TTS/image/video providers, or estimate what a render would cost. Triggers on "hypit", "SVML", "短视频", "口播视频", "克隆视频", "字幕", "卡拉OK字幕".
---

# hypit-rs 技能卡

hypit-rs 是纯 Rust 的短视频引擎：一份 `.svml` 文本描述整条影片（画布、分段口播、字幕、标题、贴纸、榜单板、背景、音效），引擎负责词级时序、光栅渲染与 ffmpeg 编码，不启动浏览器。二进制在 `hypit-rs/`（仓库子目录）下用 `cargo build -p hypit-cli` 构建，产物 `target/debug/hypit-rs`。

## 何时使用

- 用户给出一段文案/脚本，要"出一条短视频"。
- 用户给一个参考视频，要"克隆/复刻/换台词"（`transcribe`）。
- 用户要改 `.svml` 工程的文本、样式、时序或组件。
- 用户要接自己的 TTS / 图像 / 视频生成 API（BYOK），或询问"这次构建要花多少钱"（`plan`）。

## 命令速查

```sh
cd hypit-rs   # 以下均相对该目录

hypit-rs check <src.svml>                    # 解析+时序解析，打印画布/分段/图层（不渲染）
hypit-rs frame <src.svml> -n 30 -o f.png     # 渲染第 30 帧 PNG（预览；帧号从 0 起）
hypit-rs build <src.svml> -o out.mp4         # 整片渲染+编码；--approve 授权付费调用
hypit-rs plan <src.svml>                     # 构建前预估 provider 账单（缓存命中免费）
hypit-rs align <src.svml> --model m.bin      # whisper.cpp 词对齐，写 <stem>.align.json
hypit-rs transcribe <video>                  # 参考视频 → SVML 草稿 + 对齐 + 抽音轨
hypit-rs new <dir> --template talking-head   # 脚手架：talking-head/podcast/ranking/product
hypit-rs provider list|check|auth            # BYOK 端点 / 校验凭据 / 存密钥
```

全局选项：`--var k=v`（可重复）做单次变量覆盖；`build --variants v.json` 批量出片，输出模板 `-o out/{name}.mp4`。

## SVML 语法速览

```xml
<project width="1080" height="1920" fps="30">
  <script id="launch" audio="assets/voice.m4a" align="launch.align.json">
    <!-- | 分词行（词级卡拉OK的换行点）；gap-before/after 控制段间静音 -->
    <segment id="hook" gap-after="0.5s">每条口播视频|都要两小时剪辑</segment>
  </script>
  <voice provider="openai" voice="alloy" gap="0.2s"/>            <!-- BYOK 逐段 TTS -->
  <background src="assets/bg.mp4" fit="cover"/>                  <!-- 也可 generate-provider -->
  <captions y="0.78" size="58" karaoke="pop" active-fill="#0B1D3A"
            active-box="#FF6B35" active-box-radius="20"/>        <!-- 卡拉OK字幕 -->
  <title id="hookline" text="脚本即节目单" during="intro" y="0.18" enter="fade-up"/>
  <media id="hero" src="assets/logo.png" during="tech" frame="0.64,0.1,0.92,0.24"/>
  <selection id="intro" from="hook:0" to="hook:1"/>              <!-- 语义时间窗 -->
  <moment id="punch" at="cta:0"/>                                <!-- 语义时间点 -->
  <flash id="pop" at="punch" intensity="0.4"/>                   <!-- 闪白 -->
  <sound id="whoosh" src="assets/pop.m4a" at="punch"/>           <!-- 音效 -->
  <bgm id="bed" src="assets/music.m4a" gain="0.35" fade-in="1.5s"/>
</project>
```

锚点三种写法：`segment:word-index`（词锚定，首选）、`3.5s`/`360ms`/`12f`（绝对时间）、`moment-id`。`during="selection-id"` 把图层绑定到语义窗口——**改台词时图层自动跟随，无需改时间**。

组件清单：`captions`（卡拉OK/说话人分色 `speaker-colors`）、`title`、`media`（含 `column`/`chrome="window"`/`generate-provider`）、`background`、`flash`、`sound`、`bgm`、`ranking`（榜单板 `items="A|B|C" active=...`）、`sticker`（评论气泡）。

## 迭代流程（改文本 → 预览 → 出片）

1. `hypit-rs check <src.svml>`：语法/时序错误先暴露。
2. `hypit-rs frame -n <帧>`：抽查关键帧（字幕高亮、标题入场、榜单推进）。
3. `hypit-rs build -o out.mp4`：出片。改动只涉及文本/组件，不需要重跑 `align`（词数不变时对齐仍有效；**改了词数要重跑 `align`**）。
4. 预览循环期间不要反复 `build`；`frame` 秒级反馈。

## 克隆参考视频

```sh
hypit-rs transcribe ref.mp4 --language zh
# 产出 ref.svml（草稿：script+captions+背景引用）、ref.align.json、assets/voice.m4a
```

- `--pause 0.6`（默认，秒）控制停顿切段阈值；切出来的 `seg-NN` 直接改文本即可。
- 之后走常规迭代流程：改台词 → `frame` 预览 → `build` 出片。
- 诚实边界：工具提供骨架（文本、词级时间、画幅、背景）；**画面结构理解（哪里放标题、怎么排版）依赖 Agent 的多模态能力**，按参考视频观感补 `title`/`media`/`ranking` 等图层。

## BYOK 配置（TTS / 图像 / 视频生成）

1. 工程根写 `.hypit/providers.json`（端点、能力、单价表；可提交）：

```json
{ "endpoints": [ { "id": "openai", "kind": "openai",
    "capabilities": ["tts", "image"],
    "base_url": "https://api.openai.com/v1",
    "credential_ref": "OPENAI_API_KEY",
    "prices": { "tts_per_1k_chars": 0.015, "image_each": 0.04 } } ] }
```

   MiniMax 端点必须带 `"params": { "GroupId": "..." }`。
2. `hypit-rs provider auth openai` 存密钥（本机 keyring/文件，0600；同名环境变量优先）；`provider check` 校验连通。
3. `hypit-rs plan <src.svml>` 先看账单：缓存命中免费列出，缺单价的行为 `unknown`，绝不猜价。
4. `hypit-rs build ... --approve` 才会真正付费调用；未授权时 build 报错列出待付费项。生成物缓存于 `.hypit/cache/`（键 = provider+voice+model+文本 或 provider+prompt+kind），重复构建零花费；`build --reuse` 可从 `.hypit/builds/` 历史记录恢复被清掉的缓存。

## 环境注意

- 依赖系统 `ffmpeg`/`ffprobe`；whisper.cpp 对齐需 `whisper-cli` + ggml 模型（`--model` 或 `HYPIT_WHISPER_MODEL`；缺失时命令会打印安装指引而不是静默失败）。
- 测试跑 `cargo test -p hypit-cli`；改 SVML 解析/模型后跑 `cargo test -p hypit-svml`。
- 模板与完整文档：`hypit-rs/README.md`、`hypit-rs/templates/`。
