# Open Recorder

Open Recorder 是一个本地优先（local-first）的桌面录音工具，支持会后转写与会议纪要生成。

## 技术栈
- Tauri v2
- React 19 + TypeScript
- Rust（录音、存储、转写与摘要核心逻辑）

## 核心能力
- 持续录音，按 10 分钟自动切片（优先 M4A，失败回退 WAV）。
- 录音状态轮询（时长、波形 RMS/Peak）。
- 导出 M4A / MP3（均为合并后导出，MP3 依赖 `ffmpeg`）。
- 录音会话、任务、设置持久化到本地 `state.json`。
- 转写 Provider 可切换：
  - Bailian（百炼）
  - Aliyun Tingwu（听悟离线任务）
- 支持多 OSS 配置并可切换“当前生效 OSS”：
  - Aliyun OSS
  - Cloudflare R2（S3 兼容）
- 摘要使用 Bailian 兼容 Chat Completions 接口。
- 未配置 API Key 时，转写/摘要自动回退到 mock 结果。

## 数据目录
- 默认（macOS）：`~/Library/Application Support/Open Recorder`
- 可通过环境变量覆盖：`OPEN_RECORDER_DATA_DIR=/your/path`
- 关键文件：
  - `state.json`：会话、任务、设置
  - `audio/<session_id>/segments/*.{m4a,wav}`：录音切片
  - `exports/<session_id>/`：导出文件

## 开发运行
### 前置依赖
- Node.js 20+
- Rust toolchain
- Tauri 运行依赖（macOS）

### 启动开发模式
```bash
npm install
npm run tauri:dev
```

### 构建发布（macOS `.app`）
```bash
npm run tauri:build -- --bundles app
```

产物路径：
`src-tauri/target/release/bundle/macos/Open Recorder.app`

## 配置说明
应用设置页支持以下参数。

### 1) Provider（转写/摘要）
- `Transcription Provider`：从可用转写 Provider 中选择
- `Summary Provider`：从可用摘要 Provider 中选择

### 2) Bailian（百炼）
- `Bailian API Key`
- `Bailian Base URL`（默认 `https://dashscope.aliyuncs.com`）
- `Bailian Transcription Model`（默认 `paraformer-v2`）
- `Bailian Summary Model`（默认 `qwen-plus`）

### 3) Aliyun Tingwu（听悟）
- `Aliyun AccessKey ID`
- `Aliyun AccessKey Secret`
- `Aliyun Tingwu AppKey`
- `Aliyun Tingwu Endpoint`（默认 `https://tingwu.cn-beijing.aliyuncs.com`）
- `Aliyun Source Language`（`cn` 或 `en`，默认 `cn`）
- `Aliyun Language Hints`（可选，逗号分隔，例如 `cn,en`）
- `Aliyun FileUrl Prefix`（兜底公网 URL 前缀，可选）
- `NormalizationEnabled`（默认开启）
- `ParagraphEnabled`（默认开启）
- `PunctuationPredictionEnabled`（默认开启）
- `DisfluencyRemovalEnabled`（默认关闭）
- `SpeakerDiarizationEnabled`（默认开启）
- `Aliyun Poll Interval`（默认 `60` 秒，范围 `60-300`）
- `Aliyun Max Polling Time`（默认 `180` 分钟，范围 `5-720`）

### 4) OSS（多配置）
> 转写链路统一使用“当前选择 OSS”进行上传和签名 URL 生成。未完整配置时会直接报错。

- `Current OSS`：当前生效 OSS 配置
- `OSS Provider`：`aliyun` 或 `r2`
- `OSS AccessKey ID`
- `OSS AccessKey Secret`
- `OSS Endpoint`
  - Aliyun 示例：`https://oss-cn-beijing.aliyuncs.com`
  - R2 示例：`https://<accountid>.r2.cloudflarestorage.com`
- `OSS Bucket`
- `OSS Path Prefix`（默认 `open-recorder`）
- `OSS Signed URL TTL`（默认 `1800` 秒）

## 转写流程说明
### Bailian
1. 读取本地切片 `segment-*.wav`
2. 使用当前 OSS 上传分片
3. 生成签名下载 URL
4. 调用百炼转写接口（异步）
5. 轮询任务结果并回填 transcript

### Aliyun Tingwu
1. 使用当前 OSS 上传本地分片并生成签名 URL
2. 组装 `FileUrl`（优先签名 URL）
3. 创建离线任务
4. 轮询任务状态
5. 解析结果文本

说明：`QueryTaskInfo` 轮询已按听悟文档建议调整为可配置频率（默认 60 秒），并优先解析 `Data.Result.Transcription` 结果地址。

## 常见问题排查
### 1) `input must contain file_urls`
原因：请求体未携带可访问的 `file_urls`。  
处理：检查当前选择 OSS 参数是否完整、正确。

### 2) `current user api does not support synchronous calls`
原因：账号不支持同步转写。  
处理：项目已改为异步模式；请使用最新构建版本。

### 3) `failed to upload segment ...`
常见原因：
- OSS Endpoint 错误（例如写成无效域名）
- AK/SK 无权限
- Bucket 区域与 Endpoint 不匹配

建议先确认 `OSS Endpoint` 是否为正确域名（Aliyun 示例：`https://oss-cn-beijing.aliyuncs.com`）。
如使用 R2，请确认 Endpoint 为 `https://<accountid>.r2.cloudflarestorage.com` 且 Bucket 名称正确。

### 4) 在哪里看错误详情
- 应用状态文件：`~/Library/Application Support/Open Recorder/state.json`
  - 查看 `jobs.<jobId>.error`
- 终端启动应用可看到更多运行输出：
```bash
"/path/to/Open Recorder.app/Contents/MacOS/open_recorder"
```

## 后续计划
1. 增加转写重试与退避策略。
2. 增加请求级别诊断日志（可选落盘）。
3. 将状态存储由 JSON 升级到 SQLite。
