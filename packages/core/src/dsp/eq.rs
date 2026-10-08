pub const EQ_BAND_CENTERS_HZ: [f32; 5] = [60.0, 250.0, 1000.0, 4000.0, 12000.0];
pub const EQ_GAIN_RANGE_DB: (f32, f32) = (-12.0, 12.0);
const Q: f32 = 1.0;

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

fn peaking_eq_coeffs(sample_rate_hz: f32, freq_hz: f32, gain_db: f32, q: f32) -> BiquadCoeffs {
    let a = 10f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq_hz / sample_rate_hz;
    let alpha = w0.sin() / (2.0 * q);
    let cos_w0 = w0.cos();

    let b0 = 1.0 + alpha * a;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0 - alpha * a;
    let a0 = 1.0 + alpha / a;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha / a;

    BiquadCoeffs {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

fn process_one(coeffs: &BiquadCoeffs, state: &mut BiquadState, x0: f32) -> f32 {
    let y0 = coeffs.b0 * x0 + coeffs.b1 * state.x1 + coeffs.b2 * state.x2
        - coeffs.a1 * state.y1
        - coeffs.a2 * state.y2;
    state.x2 = state.x1;
    state.x1 = x0;
    state.y2 = state.y1;
    state.y1 = y0;
    y0
}

pub struct GraphicEq {
    sample_rate_hz: f32,
    gains_db: [f32; 5],
    coeffs: [BiquadCoeffs; 5],
    state: [BiquadState; 5],
}

impl GraphicEq {
    pub fn new(sample_rate_hz: f32) -> Self {
        let gains_db = [0.0; 5];
        let coeffs = Self::compute_coeffs(sample_rate_hz, &gains_db);
        GraphicEq {
            sample_rate_hz,
            gains_db,
            coeffs,
            state: [BiquadState::default(); 5],
        }
    }

    fn compute_coeffs(sample_rate_hz: f32, gains_db: &[f32; 5]) -> [BiquadCoeffs; 5] {
        std::array::from_fn(|i| {
            peaking_eq_coeffs(sample_rate_hz, EQ_BAND_CENTERS_HZ[i], gains_db[i], Q)
        })
    }

    /// Non-finite gains are treated as 0 dB: `clamp` passes NaN through and
    /// a NaN coefficient would turn every band into silence (and defeat the
    /// `clamped == self.gains_db` short-circuit forever).
    pub fn set_gains_db(&mut self, gains_db: [f32; 5]) {
        let clamped = gains_db.map(|g| {
            if g.is_finite() {
                g.clamp(EQ_GAIN_RANGE_DB.0, EQ_GAIN_RANGE_DB.1)
            } else {
                0.0
            }
        });
        if clamped == self.gains_db {
            return;
        }
        self.gains_db = clamped;
        self.coeffs = Self::compute_coeffs(self.sample_rate_hz, &clamped);
    }

    pub fn gains_db(&self) -> [f32; 5] {
        self.gains_db
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        if self.gains_db == [0.0; 5] {
            return;
        }
        for sample in samples.iter_mut() {
            let mut x = *sample;
            for band in 0..5 {
                x = process_one(&self.coeffs[band], &mut self.state[band], x);
            }
            *sample = x;
        }
    }
}
