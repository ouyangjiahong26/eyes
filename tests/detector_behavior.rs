use eyes_lib::domain::classifier::HeadPose;
use eyes_lib::monitoring::detector::Detector;

#[test]
fn detector_trait_can_be_implemented_by_fake_for_tests() {
    struct FakeDetector {
        poses: Vec<Option<HeadPose>>,
    }

    impl Detector for FakeDetector {
        fn detect(&mut self, _rgb: &[u8], _width: u32, _height: u32) -> Option<HeadPose> {
            self.poses.pop().unwrap_or(None)
        }
    }

    let mut fake = FakeDetector {
        poses: vec![
            Some(HeadPose {
                yaw: 5.0,
                pitch: 1.0,
            }),
            None,
        ],
    };

    assert_eq!(fake.detect(&[], 0, 0), None,);
    assert_eq!(
        fake.detect(&[], 0, 0),
        Some(HeadPose {
            yaw: 5.0,
            pitch: 1.0
        }),
    );
    // 空 detector 返回 None
    assert_eq!(fake.detect(&[], 0, 0), None,);
}
