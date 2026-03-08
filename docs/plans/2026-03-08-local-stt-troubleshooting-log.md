# Open Recorder Local STT 排障记录（2026-03-08）

## Summary
本次问题链路覆盖了 Local STT 从依赖缺失、模型鉴权、首次下载卡住，到 `pyannote`/`torchcodec` 运行时兼容问题。

最终状态：
1. 本地转写链路可跑通（包含说话人分离）
2. 同一份请求文件可端到端返回 `segments` 且 `error = null`
3. Open Recorder 的 Local STT `pythonPath` 已切换到包装脚本，运行时环境固定

## 初始报错与根因
1. `No module named 'pyannote'`
- 根因：应用实际使用的 Python 环境与终端安装环境不一致。

2. `401/403 Cannot access gated repo ... pyannote/speaker-diarization-3.1`
- 根因：HuggingFace gated repo 未授权或 token 作用域不足。

3. `No module named 'faster_whisper'`
- 根因：Whisper 依赖未安装到 Local STT 实际使用解释器。

4. 转写“卡住”
- 根因：首次模型下载阶段（hf-xet）长时间停滞，UI 只显示 `本地模型转写中...`。

5. 说话人分离阶段异常（多轮）
- `AudioDecoder`/`torchcodec` 相关报错：FFmpeg 动态库解析问题
- `itertracks` 不存在：`pyannote 4.x` 输出结构变化（`DiarizeOutput`）
- `requested chunk ... samples instead of expected ...`：部分 `m4a` 片段采样对齐误差导致 diarization 裁剪失败

## 关键排障动作（按链路）
1. 固定 Local STT Python
- 读取并更新：`/Users/renx/Library/Application Support/Open Recorder/state.json`
- 将 `local_stt.pythonPath` 指向可控解释器（后续改为包装脚本）

2. 安装缺失依赖
- 安装：`pyannote.audio`、`faster-whisper`
- 逐项导入验证：`pyannote.audio`、`faster_whisper`

3. 完成 HuggingFace 鉴权
- 执行 `huggingface-cli login`
- 逐项验证 gated repo 可访问：
1. `pyannote/speaker-diarization-3.1`
2. `pyannote/segmentation-3.0`
3. `pyannote/speaker-diarization-community-1`

4. 处理“首次下载卡住”
- 识别 worker 进程仍在运行但长期无响应
- 观察 xet 日志与请求文件，定位为下载链路问题
- 在运行环境中设置 `HF_HUB_DISABLE_XET=1`，并预热下载关键模型

5. 处理 `pyannote` 运行时兼容
- 统一回到稳定组合：`pyannote.audio 4.0.4 + torch 2.9.1 + torchaudio 2.9.1 + torchvision 0.24.1 + torchcodec 0.9.1`
- 通过包装脚本注入：
1. `HF_HUB_DISABLE_XET`
2. `HF_HOME`
3. `DYLD_FALLBACK_LIBRARY_PATH`
4. `PYTHONPATH`（加载兼容补丁）

6. 修复 `torchaudio` API 差异
- 增加 `sitecustomize.py`，补齐运行时兼容方法（如 `set_audio_backend`、`list_audio_backends`）

7. 修复 worker 与 `pyannote 4.x` 输出不兼容
- 新增补丁版 worker，适配 `DiarizeOutput.speaker_diarization`
- 在遇到 `m4a` 采样不齐时自动转临时 `wav` 后重试 diarization

## 本机最终配置
1. Open Recorder 配置
- 文件：`/Users/renx/Library/Application Support/Open Recorder/state.json`
- `local_stt.pythonPath`：
`/Users/renx/Library/Application Support/Open Recorder/local-stt/python-with-open-recorder-env.sh`

2. 运行包装脚本
- `/Users/renx/Library/Application Support/Open Recorder/local-stt/python-with-open-recorder-env.sh`
- 作用：统一注入 Local STT 运行环境，并把 `local_stt_worker.py` 路由到补丁脚本。

3. 兼容补丁
- `/Users/renx/Library/Application Support/Open Recorder/local-stt/sitecustomize.py`

4. 补丁 worker
- `/Users/renx/Library/Application Support/Open Recorder/local-stt/local_stt_worker_patched.py`

## 验证结果
1. `pyannote` pipeline 加载成功
- `Pipeline.from_pretrained("pyannote/speaker-diarization-3.1")` 返回 `SpeakerDiarization`

2. 端到端请求验证成功
- 使用同一请求文件调用（模拟应用真实调用形态）
- 响应文件结果：
1. `segments` 有内容
2. `speakerId` 与 `speakerLabel` 正常
3. `error = null`

示例响应（节选）：
```json
{
  "segments": [
    {
      "startMs": 0,
      "endMs": 10592,
      "text": "一二三十五六七 ABCDEFG 今天天气怎么样",
      "confidence": 0.40221097158349084,
      "speakerId": "SPEAKER_00",
      "speakerLabel": "Speaker SPEAKER_00"
    }
  ],
  "error": null
}
```

## 经验与建议
1. Local STT 必须先确定“实际执行的 Python”
- 不要只看终端 `python3`；优先看应用配置中的 `pythonPath`。

2. `pyannote` 依赖 gated repo 不止一个
- 至少要确认 `speaker-diarization-3.1`、`segmentation-3.0`、`speaker-diarization-community-1` 都可访问。

3. 首次下载需要“可观察性”
- 只看 UI 文案不足以判断卡死；要结合 worker 进程、请求/响应文件、缓存日志排查。

4. 对于桌面应用进程，环境变量继承不可假设
- 通过单一 `pythonPath` 包装脚本收敛运行环境，稳定性明显更高。

5. `m4a` + diarization 在某些短片段上可能出现采样对齐误差
- 对异常片段自动转 `wav` 再做 diarization 是实用的兜底策略。

6. 安全建议
- 调试时如暴露了 HF token，排障完成后应立即在 HuggingFace 后台轮换 token。

## 后续可选改进
1. 将补丁 worker 的兼容逻辑合并回仓库 `src-tauri/python/local_stt_worker.py`
2. 为 Local STT 增加更细粒度进度状态（下载模型、加载模型、转写、分离说话人）
3. 在设置页增加“一键本地环境自检”（Python/依赖/HF 授权/模型缓存）
