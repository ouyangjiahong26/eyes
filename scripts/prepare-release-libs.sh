#!/usr/bin/env bash
#
# scripts/prepare-release-libs.sh — 从 ort 构建产物暂存 ONNX Runtime .so 到 lib/
#
# 用途：
#   cargo bundle --format deb/rpm 打包时会读取 [package.metadata.bundle].resources
#   声明的目录（包含 lib/），把里面的文件一并塞进 deb/rpm。
#   Linux 上 ONNX Runtime 是动态库，必须随包分发；它的 .so 由 ort crate 在
#   构建时下载并展开到 target/release/build/ort-sys-*/out/（或 ort-*/out/），
#   需要本脚本集中拷贝到 lib/，供 cargo bundle 拾取。
#
# 调用：
#   scripts/prepare-release-libs.sh
#
# 前置条件：
#   至少成功跑过一次 `cargo build --release --features onnx-detector`，
#   ort crate 才会下载并展开 ONNX Runtime 二进制。
#
# 行为：
#   - 在三处候选位置搜索 libonnxruntime.so*（含带版本号的 .so.21 等）：
#       1) target/release/build/ort-sys-*/out/
#       2) target/release/build/ort-*/out/
#       3) target/release/ 顶层
#   - 复制到 lib/，保留符号链接（避免实体 .so 被复制 N 份）
#   - lib/ 不存在则自动创建
#   - 多次运行幂等：cp -P -f 覆盖
#   - 找不到任何 .so 时输出明确错误并退出非零
set -euo pipefail

cd "$(dirname "$0")/.."
PROJECT_ROOT="$(pwd)"
DEST="${PROJECT_ROOT}/lib"

mkdir -p "${DEST}"

# 收集所有候选路径：优先 ort-sys（ort 2.x 的实际构建产物所在），
# 兼容早期 ort 直接产出，以及 build script 把 .so 复制到 target/release/ 顶层的情况。
declare -a SOURCES=()
for pattern in \
    "${PROJECT_ROOT}/target/release/build/ort-sys-"*/out/libonnxruntime.so* \
    "${PROJECT_ROOT}/target/release/build/ort-"*/out/libonnxruntime.so* \
    "${PROJECT_ROOT}/target/release/libonnxruntime.so"* ; do
    # 未匹配的 glob 在 bash 中会作为字面量传入，[[ -e ]] 会判定不存在，直接跳过
    if [[ -e "${pattern}" ]]; then
        SOURCES+=("${pattern}")
    fi
done

if [[ ${#SOURCES[@]} -eq 0 ]]; then
    cat >&2 <<EOF
[错误] 未找到 libonnxruntime.so*。

请先在启用 onnx-detector feature 的前提下跑一次 release 构建：
    cargo build --release --features onnx-detector,opencv-camera

ort crate 在构建时会下载 ONNX Runtime 并展开到
target/release/build/ort{,-sys}-*/out/ 目录下，本脚本依赖该产物。
EOF
    exit 1
fi

for src in "${SOURCES[@]}"; do
    echo "[copy] ${src} -> ${DEST}/"
    # -P 保留符号链接，避免 .so.21 -> .so.21.0 被实化成多份实体文件
    cp -P -f "${src}" "${DEST}/"
done

echo
echo "=== lib/ 内容 ==="
ls -la "${DEST}/"