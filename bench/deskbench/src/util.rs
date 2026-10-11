use std::time::Instant;

pub fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

pub fn median(v: &[f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = s.len();
    if n % 2 == 1 { s[n / 2] } else { (s[n / 2 - 1] + s[n / 2]) / 2.0 }
}

pub fn min(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::INFINITY, f64::min)
}

pub fn fmt_ms(v: f64) -> String {
    if v.is_nan() {
        "-".into()
    } else if v < 10.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.1}")
    }
}

pub fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn err_str(e: &windows::core::Error) -> String {
    format!("0x{:08X} {}", e.code().0 as u32, e.message().trim())
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Frame {
    pub fn force_opaque(&mut self) {
        for px in self.bgra.chunks_exact_mut(4) {
            px[3] = 255;
        }
    }

    pub fn non_black_pct(&self) -> f64 {
        let mut nb = 0usize;
        let mut n = 0usize;
        for px in self.bgra.chunks_exact(4).step_by(97) {
            n += 1;
            if px[0] > 8 || px[1] > 8 || px[2] > 8 {
                nb += 1;
            }
        }
        if n == 0 { 0.0 } else { nb as f64 * 100.0 / n as f64 }
    }
}
