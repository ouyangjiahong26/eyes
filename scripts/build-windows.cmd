@echo off
setlocal enabledelayedexpansion
REM
REM VS7 — Windows MSI 打包脚本（Bevy 版）
REM
REM 流程：
REM   1. cargo build --release（默认 feature，不含 OpenCV/ONNX 绑定编译）
REM   2. 把 model + 运行时 DLL 拷到 target\release\（与 exe 同目录）
REM   3. cargo wix 产出 MSI
REM
REM 前置条件（机器环境，本脚本不负责安装）：
REM   - cargo-wix：cargo install cargo-wix
REM   - WiX Toolset v3：https://wixtoolset.org/releases/
REM     并把 bin 加入 PATH（candle.exe / light.exe）
REM   - 真实的 model 与 DLL 文件（仓库内是 gitignore 的占位空文件）：
REM       models\face_detection_yunet_2023mar.onnx
REM       onnxruntime.dll
REM       opencv_world4100.dll
REM     放在仓库根目录；本脚本会把它们拷到 target\release\。
REM
REM 用法：
REM   scripts\build-windows.cmd

echo === 构建 Eyes Windows MSI ===

REM --- 校验前置文件 ---
if not exist "models\face_detection_yunet_2023mar.onnx" (
    echo [错误] 缺少 models\face_detection_yunet_2023mar.onnx
    exit /b 1
)
if not exist "onnxruntime.dll" (
    echo [错误] 缺少 onnxruntime.dll（放在仓库根目录）
    exit /b 1
)
if not exist "opencv_world4100.dll" (
    echo [错误] 缺少 opencv_world4100.dll（放在仓库根目录）
    exit /b 1
)

REM --- 校验 WiX 工具链 ---
where candle.exe >nul 2>&1
if !errorlevel! neq 0 (
    echo [错误] 未找到 candle.exe，请安装 WiX Toolset v3 并加入 PATH。
    exit /b 1
)
cargo wix --version >nul 2>&1
if !errorlevel! neq 0 (
    echo [错误] 未安装 cargo-wix：cargo install cargo-wix
    exit /b 1
)

REM --- 1. 构建 release ---
echo.
echo [1/3] cargo build --release
cargo build --release
if !errorlevel! neq 0 (
    echo === 构建失败 ===
    exit /b 1
)

REM --- 2. 拷贝资源到 target 输出目录 ---
echo.
echo [2/3] 拷贝 model 与 DLL 到 target\release\
copy /Y "models\face_detection_yunet_2023mar.onnx" "target\release\face_detection_yunet_2023mar.onnx" >nul
if !errorlevel! neq 0 (
    echo [错误] 拷贝模型失败
    exit /b 1
)
copy /Y "onnxruntime.dll" "target\release\onnxruntime.dll" >nul
copy /Y "opencv_world4100.dll" "target\release\opencv_world4100.dll" >nul

REM 校验 DLL 非占位空文件
for %%F in (target\release\onnxruntime.dll) do (
    if %%~zF==0 echo [警告] onnxruntime.dll 是空文件，DLL 可能未正确打包
)
for %%F in (target\release\opencv_world4100.dll) do (
    if %%~zF==0 echo [警告] opencv_world4100.dll 是空文件，DLL 可能未正确打包
)

REM --- 3. cargo wix 产出 MSI ---
echo.
echo [3/3] cargo wix
cargo wix --no-build
if !errorlevel! neq 0 (
    echo === cargo wix 失败 ===
    exit /b 1
)

echo.
echo === 打包完成 ===
echo MSI 位于 target\wix\
