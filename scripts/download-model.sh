#!/bin/bash
# 下载 SenseVoice Small int8 模型到 data/models/（ASR 能力所需，约 230MB）
set -euo pipefail
cd "$(dirname "$0")/.."

DIR="data/models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17"
URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"

if [ -f "$DIR/model.int8.onnx" ]; then
  echo "模型已存在：$DIR"
  exit 0
fi

mkdir -p data/models
echo "下载模型（约 230MB）..."
curl -L --fail -o data/models/sv.tar.bz2 "$URL"
tar xjf data/models/sv.tar.bz2 -C data/models/
rm data/models/sv.tar.bz2
echo "完成：$DIR"
