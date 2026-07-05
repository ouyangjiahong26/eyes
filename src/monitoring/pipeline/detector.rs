use crate::domain::classifier::HeadPose;

/// 检测与监测 worker 之间的接口。
/// 后续 M4 实现 OnnxDetector，测试中用 FakeDetector。
pub trait Detector: Send + 'static {
    /// 检测到人脸返回 `Some(HeadPose)`，否则返回 `None`。
    fn detect(&mut self, rgb: &[u8], width: u32, height: u32) -> Option<HeadPose>;
}
