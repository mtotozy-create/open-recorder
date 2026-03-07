# Open Recorder

Open Recorder 是一个本地优先（local-first）的桌面录音工具，基于 Tauri + React + Rust，支持录音、导出、转写和会议纪要生成。

## 技术栈
- Tauri v2
- React 19 + TypeScript + Vite
- Rust（录音、存储、转写与摘要核心逻辑）

## 核心能力
- 本地持续录音，按 **2 分钟**自动切片。
- 录音质量可选：
  - Standard（16k 单声道）
  - HD（24k 单声道）
  - Hi-Fi（48k 双声道）
- 实时状态与波形展示（时长、RMS、Peak）。
- 会话管理：重命名、删除、查看音频分段与任务状态。
- 支持从本地音频文件导入创建会话（如 wav/m4a/mp3/aac/flac/ogg/opus/webm/mp4/m4b）。
- 导出音频：M4A / MP3（按会话合并后导出）。
- 转写 Provider 可切换：
  - Bailian（百炼）
  - Aliyun Tingwu（听悟离线任务）
- 摘要 Provider 可切换：
  - Bailian（兼容 Chat Completions）
  - OpenRouter（Chat Completions）
- 支持多 OSS 配置并选择“当前生效 OSS”：
  - Aliyun OSS
  - Cloudflare R2（S3 兼容）
- 支持摘要模板（Prompt Templates）与默认模板配置。

## 项目结构
- `src/`：前端界面与调用层（React + TS）
- `src-tauri/`：Rust 后端命令、录音与 Provider 实现
- `docs/`：设计与规划文档

## 开发运行
### 前置依赖
- Node.js 20+
- Rust toolchain
- Tauri 运行依赖
- `ffmpeg`（建议安装；MP3 导出与部分音频转换依赖）

### 启动桌面开发模式
```bash
npm install
npm run tauri:dev
```

### 仅启动前端（Web）
```bash
npm run dev
```

默认端口：`1420`

### 构建
```bash
npm run tauri:build
```

macOS `.app` 产物通常位于：
`src-tauri/target/release/bundle/macos/Open Recorder.app`

## NPM Scripts
- `npm run dev`：启动 Vite 前端开发服务
- `npm run build`：构建前端静态资源
- `npm run preview`：预览前端构建产物
- `npm run tauri:dev`：启动 Tauri 桌面开发模式
- `npm run tauri:build`：构建 Tauri 桌面应用

## 配置说明
设置页主要分为 General / Provider / OSS / Templates / About。

### 1) Provider
- `Transcription Provider`：选择当前转写 Provider
- `Summary Provider`：选择当前摘要 Provider

#### Bailian
- `API Key`
- `Base URL`（默认 `https://dashscope.aliyuncs.com`）
- `Transcription Model`（默认 `paraformer-v2`）
- `Summary Model`（默认 `qwen-plus`）

#### Aliyun Tingwu
- `AccessKey ID`
- `AccessKey Secret`
- `AppKey`
- `Endpoint`（默认 `https://tingwu.cn-beijing.aliyuncs.com`）
- `Source Language`（`cn` / `en`）
- `Language Hints`（可选，逗号分隔）
- `FileUrl Prefix`（兜底前缀，可选）
- `NormalizationEnabled`
- `ParagraphEnabled`
- `PunctuationPredictionEnabled`
- `DisfluencyRemovalEnabled`
- `SpeakerDiarizationEnabled`
- `Poll Interval`（秒，`60-300`）
- `Max Polling Time`（分钟，`5-720`）

#### OpenRouter（摘要）
- `API Key`
- `Base URL`（默认 `https://openrouter.ai/api/v1`）
- `Summary Model`（默认 `qwen/qwen-plus`）

### 2) OSS（多配置）
> 转写链路统一使用“当前选择 OSS”进行上传与签名 URL 生成。

- `Current OSS`：当前生效 OSS
- `OSS Provider`：`aliyun` / `r2`
- `AccessKey ID`
- `AccessKey Secret`
- `Endpoint`
  - Aliyun 示例：`https://oss-cn-beijing.aliyuncs.com`
  - R2 示例：`https://<accountid>.r2.cloudflarestorage.com`
- `Bucket`
- `Path Prefix`（默认 `open-recorder`）
- `Signed URL TTL`（秒，`60-86400`）

### 3) Templates
- 管理摘要模板：`Template ID` / `Template Name`
- 配置 `System Prompt` / `User Prompt` / `Variables`
- 选择默认模板 `Default Template ID`

## 数据存储
应用状态与音频文件默认落在本地目录。

数据目录候选顺序：
1. `OPEN_RECORDER_DATA_DIR`（若设置）
2. macOS：`~/Library/Application Support/Open Recorder`
3. `~/.open-recorder-data`
4. `<项目目录>/.open-recorder-data`
5. 系统临时目录下的 `open-recorder-data`

关键文件结构：
- `state.json`：会话、任务、设置
- `audio/<session_id>/segments/`：录音切片或导入音频
- `exports/<session_id>/`：导出与合并后的音频

## 转写与摘要流程
### 转写
1. 读取会话音频（优先已有导出音频，否则按分段处理）
2. 必要时自动合并音频
3. 使用当前 OSS 上传并生成签名 URL
4. 调用所选转写 Provider（Bailian / Tingwu）
5. 轮询任务并回写 transcript

### 摘要
1. 读取 transcript
2. 读取当前摘要 Provider 与模板
3. 调用 Chat Completions 接口生成结构化摘要
4. 回写 `title/decisions/actionItems/risks/timeline/rawMarkdown`

## 常见问题
### 1) `selected OSS config ... is incomplete`
原因：当前 OSS 配置缺少必填参数（AK/SK、Endpoint、Bucket）。
处理：在设置页补全当前 OSS 配置。

### 2) `provider ... requires API key`
原因：所选 Provider 未配置 API Key。
处理：在设置页补全对应 Provider 的 API Key。

### 3) `session is still processing segments`
原因：录音刚停止，后处理未完成。
处理：稍后重试转写/导出。

### 4) `failed to run ffmpeg for export`
原因：本机缺少 `ffmpeg` 或不可执行。
处理：安装并确认 `ffmpeg` 可在命令行执行。

### 5) 查看任务错误详情
- 数据文件：`state.json` 中的 `jobs.<jobId>.error`
- 前端任务面板：会话详情页 `Tasks` 标签

## 当前状态
- 当前版本：`0.1.0`
- 主要场景：本地会议录音、转写、纪要产出
- 后续可扩展方向：重试/退避策略、更细粒度诊断日志、状态存储升级（如 SQLite）
