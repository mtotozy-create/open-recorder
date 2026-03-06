# Open Recorder v1 设计方案（本地连续录音 + 录后转写 + 会议纪要）

## Summary
目标是做一个 **macOS 首发、离线优先** 的本地录音软件：
1. 稳定连续录音（长时不中断、可恢复）
2. 录音完成后调用 **阿里云百炼** 做转写
3. 基于转写结果调用大模型生成固定结构会议纪要
4. 支持用户配置自定义提示词（摘要阶段）

技术栈：`Tauri v2 + React 19 + TypeScript`，核心任务在 `Rust` 层执行，前端负责交互和状态展示。

## Scope（v1）
- 包含：
1. 本地连续录音（开始/暂停/停止）
2. 录音会话管理（列表、详情、状态）
3. 录后转写（百炼）
4. 纪要总结（百炼，固定模板）
5. 提示词配置（系统默认 + 用户自定义）
6. 本地存储（音频、转写文本、摘要结果、配置）
- 不包含：
1. 实时转写
2. 本地模型接入（作为 v1.1+）
3. Windows 支持（作为 v1.1+）
4. 多人协作/云端同步

## Architecture
1. `Tauri Rust Core`
- Audio Capture：系统麦克风采集、分段写盘（滚动切片）
- Session Manager：会话生命周期与状态机
- Job Queue：转写/摘要异步任务、重试与取消
- Provider Adapter（v1 仅 Bailian）：统一调用接口，后续可扩展本地模型/其他云
- Secure Config：API Key 安全存储（系统钥匙串 + 本地引用）

2. `React 19 + TypeScript UI`
- Recorder 页面：录音控制 + 电平/时长显示 + 当前状态
- Sessions 页面：会话列表、搜索、过滤、任务状态
- Session Detail：音频回放、转写文本、纪要结果、重新生成
- Settings：百炼配置、提示词模板、默认摘要模板

3. `Data Storage (Local)`
- 元数据：SQLite（建议）
- 音频文件：`app_data/audio/<session_id>/segments/*.wav`
- 导出文件：`txt/md/json`

## Public APIs / Interfaces / Types
1. Tauri Commands（前端调用 Rust）
- `recorder.start(inputDeviceId?) -> { sessionId }`
- `recorder.pause(sessionId) -> void`
- `recorder.resume(sessionId) -> void`
- `recorder.stop(sessionId) -> void`
- `session.list(query) -> SessionSummary[]`
- `session.get(sessionId) -> SessionDetail`
- `transcribe.enqueue(sessionId, options) -> JobId`
- `summary.enqueue(sessionId, templateId, promptOverride?) -> JobId`
- `job.get(jobId) -> JobStatus`
- `settings.get() / settings.update(partial)`

2. Core Types
- `SessionStatus = recording | paused | stopped | transcribing | summarizing | completed | failed`
- `TranscriptSegment = { startMs, endMs, text, confidence? }`
- `SummaryResult = { title, decisions[], actionItems[], risks[], timeline[], rawMarkdown }`
- `PromptTemplate = { id, name, systemPrompt, userPrompt, variables[] }`

3. Provider Adapter（抽象但 v1 单实现）
- `transcribe(audioPath, langHint?) -> TranscriptSegment[]`
- `summarize(transcriptText, promptTemplate, context?) -> SummaryResult`

## Data Flow
1. 用户点击开始录音 -> Rust 创建 `session` + 写入首段音频文件
2. 录音过程中按时长滚动切片（例如 30s/段）并持续落盘
3. 停止录音 -> 会话状态切到 `stopped`
4. 用户手动触发“转写” -> Job Queue 调用百炼转写 -> 写回 transcript
5. 用户触发“生成纪要” -> 使用模板 + 自定义提示词 -> 调用百炼 -> 写回 summary
6. 用户可导出结果（Markdown/TXT/JSON）

## Error Handling / Edge Cases
1. 录音设备被拔出：自动暂停并提示切换设备
2. 磁盘空间不足：提前阈值告警（如 < 1GB），阻止新会话开始
3. 云接口失败：指数退避重试（3 次），保留失败原因可重跑
4. 网络中断：任务进入 `failed_retryable`，UI 一键重试
5. API Key 无效：在设置页阻断并给出校验结果
6. 长会话（>4h）：分段文件 + 流式拼接转写任务，避免单文件超限

## Security / Privacy
1. 默认本地优先：音频与文本均本地保存
2. 仅在用户手动触发转写/摘要时上传必要数据
3. API Key 存系统安全存储（macOS Keychain），不明文落库
4. 日志默认脱敏（隐藏 key、截断文本内容）

## Testing & Acceptance Criteria
1. 单元测试
- 会话状态机转换正确
- 任务队列重试/取消逻辑
- Provider adapter 错误映射
2. 集成测试
- 从开始录音到纪要生成完整链路成功
- 网络失败后可重试恢复
- 大文件长时会话不崩溃
3. 端到端（手工 + 自动）
- 30 分钟连续录音稳定
- 停止后 5 分钟内得到可读纪要（取决于网络与模型时延）
- 导出文件可被常见编辑器打开
4. 验收标准（v1）
- `P0`：录音稳定、转写成功率高、纪要结构稳定
- `P1`：提示词可配置并生效
- `P2`：失败可诊断、可重试、无数据丢失

## Milestones
1. M1（基础骨架）
- Tauri 命令通道、录音引擎、会话存储
2. M2（转写能力）
- 百炼接入、任务队列、状态展示
3. M3（纪要与提示词）
- 固定模板摘要、自定义提示词、导出
4. M4（稳定性收尾）
- 异常恢复、长录音压测、打包发布（macOS）

## Assumptions & Defaults
1. 初版单用户本地应用，无账号系统
2. 首发 macOS，Windows 后续迭代
3. v1 只接阿里云百炼；本地模型接口只保留抽象不实现
4. 实时转写不纳入 v1，优先保证录后摘要质量
5. 纪要默认固定结构模板，支持提示词覆盖但不做可视化流程编排
