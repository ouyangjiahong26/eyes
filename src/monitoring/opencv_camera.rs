#[cfg(feature = "opencv-camera")]
use opencv::{core, imgproc, prelude::*, videoio};

use super::{preview::Frame, worker::FrameSource};

#[cfg(feature = "opencv-camera")]
pub struct OpenCvCamera {
    capture: videoio::VideoCapture,
}

#[cfg(feature = "opencv-camera")]
impl OpenCvCamera {
    pub fn open(index: i32) -> Result<Self, String> {
        let capture = videoio::VideoCapture::new(index, videoio::CAP_ANY)
            .map_err(|error| error.to_string())?;
        if !capture.is_opened().map_err(|error| error.to_string())? {
            return Err(format!("摄像头 {index} 不可用"));
        }
        Ok(Self { capture })
    }
}

#[cfg(feature = "opencv-camera")]
impl FrameSource for OpenCvCamera {
    fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        let mut bgr = core::Mat::default();
        let read = self
            .capture
            .read(&mut bgr)
            .map_err(|error| error.to_string())?;
        if !read || bgr.empty() {
            return Ok(None);
        }

        let mut rgb = core::Mat::default();
        imgproc::cvt_color(&bgr, &mut rgb, imgproc::COLOR_BGR2RGB, 0)
            .map_err(|error| error.to_string())?;
        let size = rgb.size().map_err(|error| error.to_string())?;
        let bytes = rgb
            .data_bytes()
            .map_err(|error| error.to_string())?
            .to_vec();
        let width =
            u32::try_from(size.width).map_err(|_| "摄像头帧宽度无效".to_string())?;
        let height =
            u32::try_from(size.height).map_err(|_| "摄像头帧高度无效".to_string())?;
        Frame::rgb(width, height, bytes)
            .map(Some)
            .map_err(|error| format!("摄像头帧无效: {error:?}"))
    }
}
