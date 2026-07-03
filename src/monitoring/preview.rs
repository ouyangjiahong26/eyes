use base64::{engine::general_purpose, Engine};
use png::{BitDepth, ColorType, Encoder};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Frame {
    pub fn rgb(width: u32, height: u32, rgb: Vec<u8>) -> Result<Self, FrameError> {
        if width == 0 || height == 0 {
            return Err(FrameError::InvalidDimensions { width, height });
        }
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or(FrameError::DimensionOverflow { width, height })?;
        if rgb.len() != expected_len {
            return Err(FrameError::InvalidRgbLength {
                expected: expected_len,
                actual: rgb.len(),
            });
        }
        Ok(Self { width, height, rgb })
    }

    /// 返回水平翻转的新帧，不修改原帧。
    pub fn mirror_horizontal(&self) -> Self {
        let row_bytes = self.width as usize * 3;
        let mut flipped = Vec::with_capacity(self.rgb.len());
        for row in self.rgb.chunks_exact(row_bytes) {
            // 逐像素反转：每 3 字节一个 RGB 像素
            for pixel in row.rchunks_exact(3) {
                flipped.extend_from_slice(pixel);
            }
        }
        Self {
            width: self.width,
            height: self.height,
            rgb: flipped,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    InvalidDimensions { width: u32, height: u32 },
    DimensionOverflow { width: u32, height: u32 },
    InvalidRgbLength { expected: usize, actual: usize },
    EncodeFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFrame {
    pub image_data_url: String,
    pub width: u32,
    pub height: u32,
}

pub fn encode_preview(frame: &Frame) -> Result<PreviewFrame, FrameError> {
    let mut png_bytes = Vec::new();
    {
        let mut encoder = Encoder::new(&mut png_bytes, frame.width, frame.height);
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| FrameError::EncodeFailed)?;
        writer
            .write_image_data(&frame.rgb)
            .map_err(|_| FrameError::EncodeFailed)?;
    }

    Ok(PreviewFrame {
        image_data_url: format!(
            "data:image/png;base64,{}",
            general_purpose::STANDARD.encode(png_bytes)
        ),
        width: frame.width,
        height: frame.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_horizontal_swaps_left_right_pixels() {
        // 2×1 帧：左红(255,0,0) 右绿(0,255,0)
        let frame = Frame::rgb(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
        let mirrored = frame.mirror_horizontal();
        // 翻转后：左绿 右红
        assert_eq!(&mirrored.rgb[..3], &[0, 255, 0]);
        assert_eq!(&mirrored.rgb[3..6], &[255, 0, 0]);
    }

    #[test]
    fn mirror_horizontal_does_not_mutate_original() {
        let frame = Frame::rgb(2, 1, vec![255, 0, 0, 0, 255, 0]).unwrap();
        let original_rgb = frame.rgb.clone();
        let _mirrored = frame.mirror_horizontal();
        assert_eq!(frame.rgb, original_rgb);
    }

    #[test]
    fn mirror_horizontal_single_pixel_frame_unchanged() {
        let frame = Frame::rgb(1, 1, vec![42, 99, 200]).unwrap();
        let mirrored = frame.mirror_horizontal();
        assert_eq!(mirrored.rgb, vec![42, 99, 200]);
        assert_eq!(mirrored.width, 1);
        assert_eq!(mirrored.height, 1);
    }

    #[test]
    fn mirror_horizontal_multi_row_flips_each_row_independently() {
        // 2×2 帧：
        //   行 0: 红(255,0,0) 绿(0,255,0)
        //   行 1: 蓝(0,0,255) 白(255,255,255)
        let frame = Frame::rgb(
            2,
            2,
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        )
        .unwrap();
        let mirrored = frame.mirror_horizontal();
        // 翻转后：
        //   行 0: 绿 红
        //   行 1: 白 蓝
        assert_eq!(
            &mirrored.rgb,
            &[0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255]
        );
    }
}
