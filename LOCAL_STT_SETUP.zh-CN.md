# 本地转写模型部署指南（macOS）

本文用于在其他 macOS 电脑上为“词元听会”客户端准备本地转写环境和离线模型，不涉及修复开发电脑。

## 1. 安装基础依赖

打开“终端”，执行：

```bash
brew install python@3.11 ffmpeg

BASE="$HOME/Library/Application Support/Open Recorder/local-stt"
python3.11 -m venv "$BASE/venv"

"$BASE/venv/bin/python3" -m pip install --upgrade pip
"$BASE/venv/bin/python3" -m pip install \
  faster-whisper \
  huggingface-hub \
  funasr \
  pyannote.audio \
  torch \
  opencc-python-reimplemented \
  mlx-whisper
```

## 2. 下载 Whisper 模型

下面以客户端选择 `small` 模型为例：

```bash
BASE="$HOME/Library/Application Support/Open Recorder/local-stt"
CACHE="$BASE/models"
mkdir -p "$CACHE"

HF_HOME="$CACHE" "$BASE/venv/bin/python3" -c \
'from huggingface_hub import snapshot_download; snapshot_download("Systran/faster-whisper-small", local_files_only=False)'
```

如果客户端的计算设备选择 `MPS`，还需要下载对应的 MLX 模型：

```bash
BASE="$HOME/Library/Application Support/Open Recorder/local-stt"
CACHE="$BASE/models"

HF_HOME="$CACHE" "$BASE/venv/bin/python3" -c \
'from huggingface_hub import snapshot_download; snapshot_download("mlx-community/whisper-small-mlx", local_files_only=False)'
```

下载过程需要访问 Hugging Face。下载完成后，实际转写可以离线运行。

## 3. 配置客户端

进入“设置 → 本地转写”，填写以下配置：

- Python 路径：`/Users/实际用户名/Library/Application Support/Open Recorder/local-stt/venv/bin/python3`
- 虚拟环境目录：`/Users/实际用户名/Library/Application Support/Open Recorder/local-stt/venv`
- 模型缓存目录：`/Users/实际用户名/Library/Application Support/Open Recorder/local-stt/models`
- Whisper 模型：`small`
- 计算设备：
  - Intel Mac：选择 `CPU`
  - Apple Silicon：可以选择 `MPS`

必须填写展开后的绝对路径，不要在客户端输入 `$HOME`。模型缓存目录必须与下载模型时使用的 `HF_HOME` 指向同一个目录。

保存设置后，完全退出并重新启动客户端。

## 4. 验证离线缓存

执行以下命令，验证 CPU 模型可在断网状态下被程序找到：

```bash
BASE="$HOME/Library/Application Support/Open Recorder/local-stt"
CACHE="$BASE/models"

HF_HOME="$CACHE" "$BASE/venv/bin/python3" -c \
'from huggingface_hub import snapshot_download; print(snapshot_download("Systran/faster-whisper-small", local_files_only=True))'
```

如果输出包含类似下面的 `snapshots` 路径，说明缓存有效：

```text
/Users/用户名/Library/Application Support/Open Recorder/local-stt/models/hub/models--Systran--faster-whisper-small/snapshots/模型版本号
```

使用 `MPS` 时，可以继续验证 MLX 模型：

```bash
BASE="$HOME/Library/Application Support/Open Recorder/local-stt"
CACHE="$BASE/models"

HF_HOME="$CACHE" "$BASE/venv/bin/python3" -c \
'from huggingface_hub import snapshot_download; print(snapshot_download("mlx-community/whisper-small-mlx", local_files_only=True))'
```

验证成功后，在客户端重新运行转写。

## 5. 其他模型对应关系

客户端选择的模型必须与下载的模型一致。

| 客户端模型 | CPU 模型 | macOS MPS 模型 |
| --- | --- | --- |
| `small` | `Systran/faster-whisper-small` | `mlx-community/whisper-small-mlx` |
| `medium` | `Systran/faster-whisper-medium` | `mlx-community/whisper-medium-mlx` |
| `large-v3` | `Systran/faster-whisper-large-v3` | `mlx-community/whisper-large-v3-mlx` |

使用 `medium` 或 `large-v3` 时，将第 2、4 节命令中的模型 ID 替换为表格中对应的 ID。

## 注意事项

- 不要直接下载模型仓库的 ZIP 文件。必须使用 `snapshot_download`，以生成 Hugging Face 离线模式需要的缓存结构。
- 报错中出现 `outgoing traffic has been disabled` 是正常的离线保护行为；它表示所选模型尚未出现在 worker 当前使用的缓存目录中。
- 如果更改了 Whisper 模型或计算设备，需要提前下载新配置所对应的模型。
- `large-v3` 模型占用空间较大，部署前应确认磁盘容量充足。
