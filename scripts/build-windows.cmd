@echo off
setlocal enabledelayedexpansion
REM
REM Eyes — Windows MSI 打包脚本（完整功能版）
REM
REM 流程：
REM   1. cargo build --release --features opencv-camera,onnx-detector
REM   2. 把 model + 真实运行时 DLL 拷到 target\release\（与 exe 同目录）
REM   3. cargo wix 产出 MSI
REM
REM 前置条件（机器环境，本脚本不负责安装）：
REM   - cargo-wix：cargo install cargo-wix
REM   - WiX Toolset v3：https://wixtoolset.org/releases/
REM     并把 bin 加入 PATH（candle.exe / light.exe）
REM   - OpenCV 通过 scoop 安装：scoop install opencv@4.10.0
REM   - 开发过程中 ort crate 会把 onnxruntime.dll 下载到 target\debug\（或 release）
REM
REM 用法：
REM   scripts\build-windows.cmd

echo === 构建 Eyes Windows MSI ===

REM --- 校验前置文件 ---
if not exist "models\face_detection_yunet_2023mar.onnx" (
    echo [错误] 缺少 models\face_detection_yunet_2023mar.onnx
    exit /b 1
)

REM --- 校验 OpenCV DLL ---
set "OPENCV_DLL=%USERPROFILE%\scoop\apps\opencv\current\x64\vc16\bin\opencv_world4100.dll"
if not exist "%OPENCV_DLL%" (
    echo [错误] 找不到 OpenCV DLL：%OPENCV_DLL%
    echo        请通过 scoop install opencv@4.10.0 安装。
    exit /b 1
)

REM --- 定位 onnxruntime.dll ---
set "ONNX_SRC="
if exist "target\debug\onnxruntime.dll" (
    set "ONNX_SRC=target\debug\onnxruntime.dll"
) else if exist "target\release\onnxruntime.dll" (
    set "ONNX_SRC=target\release\onnxruntime.dll"
)
if "!ONNX_SRC!"=="" (
    echo [错误] 找不到 onnxruntime.dll
    echo        请先用 cargo build 编译一次（ort crate 会下载该 DLL），
    echo        或手动放到 target\debug\onnxruntime.dll / target\release\onnxruntime.dll。
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
echo [1/3] cargo build --release --features opencv-camera,onnx-detector
cargo build --release --features opencv-camera,onnx-detector
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
copy /Y "%OPENCV_DLL%" "target\release\opencv_world4100.dll" >nul
if !errorlevel! neq 0 (
    echo [错误] 拷贝 opencv_world4100.dll 失败
    exit /b 1
)
if /I "%ONNX_SRC%"=="target\release\onnxruntime.dll" (
    echo [提示] 使用 target\release\ 下已有的 onnxruntime.dll，跳过拷贝。
) else (
    copy /Y "%ONNX_SRC%" "target\release\onnxruntime.dll" >nul
    if !errorlevel! neq 0 (
        echo [错误] 拷贝 onnxruntime.dll 失败
        exit /b 1
    )
)

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
