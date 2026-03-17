<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt="Open Recorder Logo">
</p>

<h1 align="center">Open Recorder</h1>

<p align="center">
  本地优先的桌面录音工具，支持语音转写与 AI 会议纪要生成。
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="#功能特性">功能特性</a> ·
  <a href="#安装">安装</a> ·
  <a href="#快速上手">快速上手</a> ·
  <a href="#配置说明">配置说明</a> ·
  <a href="#常见问题">常见问题</a>
</p>

<p align="center">
  <img src="https://img.shields.io/github/license/mtotozy-create/open-recorder" alt="License">
  <img src="https://img.shields.io/badge/version-0.3.0-blue" alt="Version">
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey" alt="Platform">
</p>

---

## 功能特性

### 🎙️ 录音
- **本地持续录音** — 音频按 2 分钟自动切片保存
- **输入设备选择** — 可选系统默认或指定麦克风，录音中锁定设备避免中途切换
- **可选录音质量**：
  - Standard（16 kHz 单声道）
  - HD（24 kHz 单声道）
  - Hi-Fi（48 kHz 双声道）
- **实时波形展示** — 实时显示时长、RMS、Peak 等指标
- **导入音频文件** — 支持从本地音频文件创建会话（wav、m4a、mp3、aac、flac、ogg、opus、webm、mp4、m4b）

### 📝 转写
- **多转写 Provider 支持**：
  - **百炼（Bailian）** — 阿里云 DashScope
  - **阿里云听悟（Tingwu）** — 离线任务模式
  - **本地 STT** — Whisper / SenseVoiceSmall，可选说话人分离
- **实时转写** — 基于阿里云听悟 WebSocket，网络断开后自动重连（每 5 秒一次，最多 3 次）
- **实时翻译** — 支持将识别语音实时翻译为多种目标语言

### 📋 AI 纪要
- **多纪要 Provider 支持**：
  - **百炼** — 兼容 Chat Completions
  - **OpenRouter** — Chat Completions，可使用 100+ 模型
- **可自定义提示词模板** — 自定义 System / User Prompt 及变量
- **结构化输出** — 标题、决策、行动项、风险、时间线、原始 Markdown

### 📦 导出
- **导出音频** — 支持 M4A / MP3 格式（分段合并后导出）
- **一键复制纪要** — 复制到剪贴板

### 🌐 多语言 UI
- 内置支持 **English** 与 **简体中文**
- 在 设置 > 通用 页面切换语言

### ☁️ 云存储（OSS）集成
- 音频上传至云端用于转写
- 支持 **阿里云 OSS** 和 **Cloudflare R2**（S3 兼容）
- 支持多 OSS 配置并选择"当前生效 OSS"

---

## 技术栈

| 层级 | 技术 |
|------|------|
| 框架 | [Tauri v2](https://v2.tauri.app/) |
| 前端 | React 19 + TypeScript + Vite |
| 后端 | Rust（录音、存储、转写与纪要核心逻辑） |
| 音频 | [cpal](https://crates.io/crates/cpal) + [hound](https://crates.io/crates/hound) + ffmpeg |

---

## 安装

### 下载预构建包

> 🚧 预构建安装包即将发布，目前请通过源码构建。

### 从源码构建

#### 前置依赖

| 依赖 | 版本 | 说明 |
|------|------|------|
| [Node.js](https://nodejs.org/) | 20+ | 前端构建 |
| [Rust](https://rustup.rs/) | 最新稳定版 | 后端构建 |
| [Tauri CLI](https://v2.tauri.app/start/prerequisites/) | v2 | 通过 npm devDependencies 安装 |
| [ffmpeg](https://ffmpeg.org/) | 任意较新版本 | MP3 导出和音频合并需要 |

在 macOS 上可以通过 Homebrew 安装 ffmpeg：
```bash
brew install ffmpeg
```

#### 构建步骤

```bash
# 克隆项目
git clone https://github.com/mtotozy-create/open-recorder.git
cd open-recorder

# 安装前端依赖
npm install

# 开发模式启动
npm run tauri:dev

# 生产构建
npm run tauri:build
```

构建产物 `.app` 位于：
```
src-tauri/target/release/bundle/macos/Open Recorder.app
```

---

## 快速上手

### 1. 录音

1. 打开应用，进入 **录音** 页面。
2. 选择麦克风（或保持"系统默认"）。
3. 选择录音质量。
4. 点击 **开始** 开始录音，音频将本地按 2 分钟切片保存。
5. 录音完毕后点击 **停止**，系统自动创建会话。

### 2. 转写

1. 进入 **会话** 页面，选择一个会话。
2. 点击 **执行转写**。
3. 应用将自动合并音频分片（如需要），上传至已配置的 OSS（云端 Provider 需要），并提交转写任务。
4. 结果显示在 **转写与纪要** 标签下。

### 3. 生成纪要

1. 转写完成后，点击 **生成纪要**。
2. 可选择纪要模板。
3. AI 将生成结构化会议纪要（决策、行动项、风险、时间线）。
4. 点击 **复制** 将纪要复制到剪贴板。

### 4. 导出音频

- 在会话详情中，使用 **导出 M4A** 或 **导出 MP3** 导出合并后的录音。

### 5. 导入音频文件

- 在 **会话** 页面，点击 **新建会话**，选择本地音频文件导入。
- 支持格式：wav、m4a、mp3、aac、flac、ogg、opus、webm、mp4、m4b。

---

## 配置说明

所有设置位于 **设置** 页面，按分组组织：

### 通用

| 设置项 | 说明 |
|--------|------|
| 语言 | UI 语言（English / 中文） |
| 录音分片时长 | 每个音频分片的时长（默认 120 秒） |
| 麦克风 | 选择录音输入设备 |

### Provider

配置用于转写和纪要的 AI 服务：

#### 百炼（Bailian）

| 字段 | 默认值 | 说明 |
|------|--------|------|
| API Key | — | DashScope API 密钥 |
| Base URL | `https://dashscope.aliyuncs.com` | API 端点 |
| 转写模型 | `paraformer-v2` | 语音识别模型 |
| 纪要模型 | `qwen-plus` | 大语言模型 |

#### 阿里云听悟（Tingwu）

| 字段 | 默认值 | 说明 |
|------|--------|------|
| AccessKey ID / Secret | — | 阿里云凭证 |
| AppKey | — | 听悟应用 Key |
| Endpoint | `https://tingwu.cn-beijing.aliyuncs.com` | API 端点 |
| 源语言 | `cn` | `cn` / `en` / `yue` / `ja` / `ko` / `multilingual` |
| 说话人分离 | 关闭 | 识别不同说话人 |
| 轮询间隔 | 60 秒 | 轮询离线任务状态间隔（60–300 秒） |
| 最长轮询时长 | 30 分钟 | 最长等待任务完成时间（5–720 分钟） |

实时转写在 WebSocket 断开后自动重连（每 5 秒一次，最多 3 次）。

#### OpenRouter（仅纪要）

| 字段 | 默认值 | 说明 |
|------|--------|------|
| API Key | — | OpenRouter API 密钥 |
| Base URL | `https://openrouter.ai/api/v1` | API 端点 |
| 纪要模型 | `qwen/qwen-plus` | 模型标识 |

#### 本地 STT

| 字段 | 默认值 | 说明 |
|------|--------|------|
| 引擎 | `whisper` | `whisper` 或 `sensevoice_small` |
| Whisper 模型 | `small` | `small` / `medium` / `large-v3` |
| 语言 | `auto` | `auto` / `zh` / `en` |
| 说话人分离 | 关闭 | 基于 pyannote 的说话人分离 |
| 计算设备 | `auto` | `auto` / `cpu` / `mps` / `cuda` |
| Python 路径 | — | Python ≥ 3.10 解释器路径 |
| 虚拟环境目录 | — | 虚拟环境路径 |
| 模型缓存目录 | — | 模型下载缓存位置 |

### OSS（对象存储）

支持配置 **多个 OSS 条目** 并选择当前生效的配置。云端转写 Provider 会将音频上传至当前生效的 OSS。

| 字段 | 说明 |
|------|------|
| OSS 提供商 | `aliyun` 或 `r2`（Cloudflare R2） |
| AccessKey ID / Secret | 存储凭证 |
| Endpoint | `https://oss-cn-beijing.aliyuncs.com`（阿里云）或 `https://<accountid>.r2.cloudflarestorage.com`（R2） |
| Bucket | 存储桶名称 |
| 路径前缀 | 上传路径前缀（默认 `open-recorder`） |
| 签名 URL TTL | 预签名 URL 有效期（60–86400 秒） |

### 模板

管理可复用的纪要提示词模板：

- **模板 ID / 名称** — 唯一标识与显示名称
- **系统提示词** — LLM 系统指令
- **用户提示词** — 支持 `{variable}` 占位符的提示词模板
- **变量** — 可用变量列表（逗号分隔）
- **默认模板** — 设置新纪要默认使用的模板

---

## 数据存储

所有数据保存在本地。应用按以下顺序查找数据目录：

1. `OPEN_RECORDER_DATA_DIR` 环境变量（如果设置）
2. `~/Library/Application Support/Open Recorder`（macOS）
3. `~/.open-recorder-data`
4. `<项目目录>/.open-recorder-data`
5. 系统临时目录下的 `open-recorder-data`

### 目录结构

```
<data-dir>/
├── state.json                          # 会话、任务、设置
├── audio/<session_id>/segments/        # 录音分片（WAV 块）
└── exports/<session_id>/              # 导出 / 合并后的音频文件
```

---

## 常见问题

### "selected OSS config ... is incomplete"
当前 OSS 配置缺少必填字段（AccessKey ID/Secret、Endpoint 或 Bucket）。请前往 **设置 > OSS** 补全配置。

### "provider ... requires API key"
所选转写或纪要 Provider 未配置 API Key。请前往 **设置 > Provider** 输入 API Key。

### "session is still processing segments"
录音刚停止，音频后处理尚未完成。请稍候再执行转写或导出。

### "failed to run ffmpeg for export"
系统未安装 `ffmpeg` 或不在 PATH 中。使用以下方式安装：
```bash
# macOS
brew install ffmpeg
```

### "realtime websocket disconnected; retried every 5 seconds for 3 times but still failed"
阿里云听悟实时转写 WebSocket 连续重连 3 次后仍未恢复。请检查：
- 网络连通性
- 阿里云 AccessKey ID/Secret 和 AppKey 配置
- 服务可用性

### 在哪里查看任务错误详情？
- **在应用中**：打开会话 → 进入 **任务详情** 标签
- **在数据文件中**：查看 `state.json` → `jobs.<jobId>.error`

---

## 开发

### NPM 脚本

| 脚本 | 说明 |
|------|------|
| `npm run dev` | 启动 Vite 前端开发服务（仅前端） |
| `npm run build` | 构建前端静态资源 |
| `npm run preview` | 预览前端构建产物 |
| `npm run tauri:dev` | 启动 Tauri 桌面开发模式 |
| `npm run tauri:build` | 构建 Tauri 桌面应用 |
| `npm run version:sync` | 将 `package.json` 版本同步到 Tauri/Cargo/README 徽章 |
| `npm run version:check` | 检查版本文件是否一致 |
| `npm run release:dry` | 预览下一版本和 changelog（不改文件） |
| `npm run release` | 交互式发布（SemVer + Conventional Commits，内含一次 `tauri:build`） |
| `npm run release:patch` | 非交互 patch 发布（内含一次 `tauri:build`） |
| `npm run release:minor` | 非交互 minor 发布（内含一次 `tauri:build`） |
| `npm run release:major` | 非交互 major 发布（内含一次 `tauri:build`） |

### 项目结构

```
open-recorder/
├── src/                        # 前端（React + TypeScript）
│   ├── components/             # UI 组件
│   │   ├── RecorderTab.tsx     # 录音界面
│   │   ├── SessionsTab.tsx     # 会话管理
│   │   ├── SettingsTab.tsx     # 设置面板
│   │   └── TabNav.tsx          # 导航标签
│   ├── i18n/                   # 国际化
│   ├── lib/                    # 工具函数
│   ├── types/                  # TypeScript 类型定义
│   ├── App.tsx                 # 主应用
│   └── styles.css              # 全局样式
├── src-tauri/                  # 后端（Rust）
│   ├── src/
│   │   ├── commands/           # Tauri 命令处理
│   │   ├── providers/          # 转写/纪要/OSS Provider 实现
│   │   ├── models.rs           # 数据模型与领域类型
│   │   ├── storage.rs          # 本地文件存储
│   │   └── state.rs            # 应用状态管理
│   └── tauri.conf.json         # Tauri 配置
└── package.json
```

---

## 贡献

欢迎贡献！请随时提交 Pull Request。

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'feat: add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

---

## 许可证

本项目基于 [MIT 许可证](LICENSE) 开源。

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/mtotozy-create">mtotozy-create</a>
</p>
