export type LocalWhisperModelId = "small" | "medium" | "large-v3";

export type WhisperModelProfile = {
  id: LocalWhisperModelId;
  label: string;
  fasterWhisperModel: string;
  mlxWhisperModel: string;
};

export const WHISPER_MODEL_PROFILES: WhisperModelProfile[] = [
  {
    id: "small",
    label: "Small",
    fasterWhisperModel: "small",
    mlxWhisperModel: "mlx-community/whisper-small-mlx"
  },
  {
    id: "medium",
    label: "Medium",
    fasterWhisperModel: "medium",
    mlxWhisperModel: "mlx-community/whisper-medium-mlx"
  },
  {
    id: "large-v3",
    label: "Large v3",
    fasterWhisperModel: "large-v3",
    mlxWhisperModel: "mlx-community/whisper-large-v3-mlx"
  }
];

