#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseSample {
    pub yaw: f64,
    pub pitch: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationResult {
    pub yaw: f64,
    pub pitch: f64,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptySamples;

const COUNTDOWN_EPSILON: f64 = 1e-9;

pub fn compute_median_pose(samples: &[PoseSample]) -> Result<PoseSample, EmptySamples> {
    if samples.is_empty() {
        return Err(EmptySamples);
    }

    let mut sorted: Vec<PoseSample> = samples.to_vec();
    sorted.sort_by(|a, b| {
        a.yaw
            .partial_cmp(&b.yaw)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let len = sorted.len();
    if len == 1 {
        return Ok(sorted[0]);
    }

    if len % 2 == 1 {
        return Ok(sorted[len / 2]);
    }

    let mid1 = &sorted[len / 2 - 1];
    let mid2 = &sorted[len / 2];
    Ok(PoseSample {
        yaw: (mid1.yaw + mid2.yaw) / 2.0,
        pitch: (mid1.pitch + mid2.pitch) / 2.0,
    })
}

#[derive(Debug, Clone)]
pub struct CalibrationSession {
    duration_seconds: f64,
    countdown: f64,
    samples: Vec<PoseSample>,
    active: bool,
    finished: bool,
}

impl CalibrationSession {
    pub fn new(duration_seconds: f64) -> Self {
        Self {
            duration_seconds,
            countdown: 0.0,
            samples: Vec::new(),
            active: false,
            finished: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn countdown_seconds(&self) -> f64 {
        self.countdown
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn start(&mut self) {
        self.samples.clear();
        self.countdown = self.duration_seconds;
        self.active = true;
        self.finished = false;
    }

    pub fn feed(&mut self, yaw: f64, pitch: f64) {
        if !self.active {
            return;
        }
        self.samples.push(PoseSample { yaw, pitch });
    }

    pub fn tick(&mut self, dt: f64) {
        if !self.active {
            return;
        }
        self.countdown -= dt;
        if self.countdown <= COUNTDOWN_EPSILON {
            self.countdown = 0.0;
            self.active = false;
            self.finished = true;
        }
    }

    pub fn result(&self) -> Option<CalibrationResult> {
        if !self.finished || self.samples.is_empty() {
            return None;
        }
        let median = compute_median_pose(&self.samples).ok()?;
        Some(CalibrationResult {
            yaw: median.yaw,
            pitch: median.pitch,
            sample_count: self.samples.len(),
        })
    }
}

/// 喂入一个校准样本并推进 0.1 秒倒计时。
///
/// 如果会话仍在进行中，返回 `None`；校准结束时返回结果。
/// 对非活跃会话不做任何操作。
pub fn feed_calibration_sample(
    session: &mut CalibrationSession,
    yaw: f64,
    pitch: f64,
) -> Option<CalibrationResult> {
    if !session.is_active() {
        return None;
    }
    session.feed(yaw, pitch);
    session.tick(0.1);
    if session.is_active() {
        None
    } else {
        session.result()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_calibration_sample_returns_none_while_active() {
        let mut session = CalibrationSession::new(1.0);
        session.start();

        // 1 秒倒计时，每 tick 0.1 秒，前 9 次仍在进行中
        for _ in 0..9 {
            assert!(feed_calibration_sample(&mut session, 3.0, 5.0).is_none());
        }
        assert!(session.is_active());
        assert_eq!(session.sample_count(), 9);
    }

    #[test]
    fn feed_calibration_sample_returns_result_when_finished() {
        let mut session = CalibrationSession::new(1.0);
        session.start();

        // 第 10 次 tick 使倒计时归零
        for _ in 0..9 {
            feed_calibration_sample(&mut session, 3.0, 5.0);
        }
        let result = feed_calibration_sample(&mut session, 7.0, 9.0);
        let result = result.expect("校准应已完成");
        assert_eq!(result.sample_count, 10);
        // 所有样本 yaw=3.0 但最后一次 yaw=7.0，中位数为 3.0
        assert!((result.yaw - 3.0).abs() < 1e-9);
    }

    #[test]
    fn feed_calibration_sample_ignores_inactive_session() {
        let mut session = CalibrationSession::new(1.0);
        // 未 start，直接调用
        assert!(feed_calibration_sample(&mut session, 1.0, 2.0).is_none());
        assert_eq!(session.sample_count(), 0);
    }
}
