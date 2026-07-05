#!/usr/bin/env bash
#
# scripts/prepare-release-libs.sh — 从 ort 构建产物暂存 ONNX Runtime .so 到 lib/
#
# 用途：
#   cargo bundle --format deb/rpm 打包时会读取 [package.metadata.bundle].resources
#   声明的目录（包含 lib/），把里面的文件一并塞进 deb/rpm。
#   当 ONNX Runtime 是动态库时（早期 ort crate），.so 由 ort 在构建时下载并
#   展开到 target/release/build/ort-sys-*/out/，需要本脚本集中拷贝到 lib/，
#   供 cargo bundle 拾取。
#
# 调用：
#   scripts/prepare-release-libs.sh
#
# 前置条件：
#   至少成功跑过一次 `cargo build --release --features onnx-detector`。
#
# 行为：
#   - 在三处候选位置搜索 libonnxruntime.so*（含带版本号的 .so.21 等）：
#       1) target/release/build/ort-sys-*/out/
#       2) target/release/build/ort-*/out/
#       3) target/release/ 顶层
#   - 找到则复制到 lib/，保留符号链接；多次运行幂等
#   - 找不到任何 .so 时打 info 日志并退出 0（ort 2.x 默认静态链接到二进制，
#     没有动态库需要分发，lib/ 在 bundle 时为空白目录属正常情况）
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
    cat <<EOF
[info] 未找到 libonnxruntime.so*，跳过。
       ort 2.x 默认静态链接 ONNX Runtime 到二进制，无 .so 需随包分发。
       lib/ 将作为空目录随 deb/rpm 一起打包（cargo bundle resources 行为）。
EOF
    exit 0
fi

for src in "${SOURCES[@]}"; do
    echo "[copy] ${src} -> ${DEST}/"
    # -P 保留符号链接，避免 .so.21 -> .so.21.0 被实化成多份实体文件
    cp -P -f "${src}" "${DEST}/"
done

echo
echo "=== lib/ 内容 ==="
ls -la "${DEST}/"