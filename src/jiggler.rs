use crate::cli::{Config, Mode};
use crate::platform::Mouse;
use std::time::{Duration, Instant};

pub fn run(mouse: &Mouse, cfg: &Config) -> Result<(), String> {
    let start = Instant::now();
    let mut rng = SmallRng::seed_from_clock();
    let log = !cfg.quiet;

    if log {
        let runtime = match cfg.max_runtime {
            Some(d) => format!(" max-runtime={}s", d.as_secs()),
            None => String::new(),
        };
        eprintln!(
            "mouse-jiggler: mode={:?} interval={}s distance={}px{}{}",
            cfg.mode,
            cfg.interval.as_secs(),
            cfg.distance,
            if cfg.once { " once=true" } else { "" },
            runtime,
        );
    }

    let mut iter: u64 = 0;
    loop {
        if let Some(max) = cfg.max_runtime {
            if start.elapsed() >= max {
                break;
            }
        }

        if cfg.verbose {
            eprintln!("[{iter}] jiggle");
        }

        match cfg.mode {
            Mode::Pixel => pixel(mouse, cfg.distance)?,
            Mode::Circle => circle(mouse, cfg.distance)?,
            Mode::Random => random(mouse, cfg.distance, &mut rng)?,
        }

        iter += 1;
        if cfg.once {
            break;
        }
        std::thread::sleep(cfg.interval);
    }
    Ok(())
}

fn pixel(m: &Mouse, d: i32) -> Result<(), String> {
    m.move_relative(d, 0)?;
    std::thread::sleep(Duration::from_millis(50));
    m.move_relative(-d, 0)
}

fn circle(m: &Mouse, r: i32) -> Result<(), String> {
    let steps = 8;
    let (mut prev_x, mut prev_y) = (0.0_f64, 0.0_f64);
    for i in 0..=steps {
        let theta = (i as f64) * std::f64::consts::TAU / (steps as f64);
        let x = (r as f64) * theta.cos() - (r as f64); // start and end at origin
        let y = (r as f64) * theta.sin();
        let dx = (x - prev_x).round() as i32;
        let dy = (y - prev_y).round() as i32;
        if dx != 0 || dy != 0 {
            m.move_relative(dx, dy)?;
        }
        prev_x = x;
        prev_y = y;
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn random(m: &Mouse, d: i32, rng: &mut SmallRng) -> Result<(), String> {
    let dx = rng.range_inclusive(-d, d);
    let dy = rng.range_inclusive(-d, d);
    if dx == 0 && dy == 0 {
        // ensure we actually moved
        m.move_relative(d, 0)?;
        std::thread::sleep(Duration::from_millis(50));
        return m.move_relative(-d, 0);
    }
    m.move_relative(dx, dy)?;
    std::thread::sleep(Duration::from_millis(50));
    m.move_relative(-dx, -dy)
}

// Tiny LCG so we don't pull in `rand`. Deterministic period of 2^64.
struct SmallRng {
    state: u64,
}

impl SmallRng {
    fn seed_from_clock() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEAD_BEEF_CAFE_BABE);
        Self {
            state: nanos
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407),
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
    fn range_inclusive(&mut self, lo: i32, hi: i32) -> i32 {
        let span = (hi - lo + 1) as u64;
        if span == 0 {
            return lo;
        }
        lo + (self.next_u64() % span) as i32
    }
}
