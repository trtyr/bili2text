#!/bin/bash
# 下载 SenseVoice Small int8 模型到 ~/.local/share/bili2text/models/（ASR 能力所需）
set -euo pipefail

DATA_DIR="${HOME}/.local/share/bili2text"
DIR="${DATA_DIR}/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"

if [ -f "$DIR/model.int8.onnx" ]; then
  echo "模型已存在：$DIR"
  exit 0
fi

mkdir -p "${DATA_DIR}/models"
echo "下载模型（压缩包约 230MB）..."
curl -L --fail -o "${DATA_DIR}/models/sv.tar.bz2" "$URL"
tar xjf "${DATA_DIR}/models/sv.tar.bz2" -C "${DATA_DIR}/models/"
rm "${DATA_DIR}/models/sv.tar.bz2"
echo "完成：$DIR"
