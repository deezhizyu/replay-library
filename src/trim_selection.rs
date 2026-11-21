const MINIMUM_LENGTH_SECONDS: f64 = 0.2;
const RULER_STEPS: [f64; 9] = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0];
const RULER_TARGET_MARKS: f64 = 9.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrimSelection {
    pub start: f64,
    pub end: f64,
    duration: f64,
}

impl TrimSelection {
    pub fn whole(duration: f64) -> Self {
        Self {
            start: 0.0,
            end: duration,
            duration,
        }
    }

    pub fn length(&self) -> f64 {
        self.end - self.start
    }

    pub fn set_start(&mut self, seconds: f64) {
        self.start = seconds.clamp(0.0, (self.end - MINIMUM_LENGTH_SECONDS).max(0.0));
    }

    pub fn set_end(&mut self, seconds: f64) {
        self.end = seconds.clamp(
            (self.start + MINIMUM_LENGTH_SECONDS).min(self.duration),
            self.duration,
        );
    }

    pub fn fractions(&self) -> (f32, f32) {
        let total = self.duration.max(f64::EPSILON);
        ((self.start / total) as f32, (self.end / total) as f32)
    }
}

pub fn ruler_marks(duration: f64) -> Vec<f64> {
    let step = RULER_STEPS
        .iter()
        .copied()
        .find(|step| duration / step <= RULER_TARGET_MARKS)
        .unwrap_or(RULER_STEPS[RULER_STEPS.len() - 1]);
    (0..)
        .map(|index| f64::from(index) * step)
        .take_while(|mark| *mark <= duration)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_never_cross_and_keep_a_minimum_length() {
        let mut selection = TrimSelection::whole(40.0);
        selection.set_start(12.3);
        selection.set_end(31.8);
        assert!((selection.length() - 19.5).abs() < 1e-9);
        selection.set_start(35.0);
        assert!((selection.start - (31.8 - MINIMUM_LENGTH_SECONDS)).abs() < 1e-9);
        selection.set_end(0.0);
        assert!((selection.end - (selection.start + MINIMUM_LENGTH_SECONDS)).abs() < 1e-9);
    }

    #[test]
    fn ruler_marks_scale_with_the_duration() {
        assert_eq!(
            ruler_marks(42.0),
            vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0]
        );
        assert_eq!(ruler_marks(5.0).len(), 6);
        assert!(ruler_marks(3_600.0).len() <= 13);
    }
}
