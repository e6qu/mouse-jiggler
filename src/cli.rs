use std::time::Duration;

pub const HELP: &str = "\
mouse-jiggler — keep the cursor (and your session) alive

USAGE:
    mouse-jiggler [OPTIONS]

OPTIONS:
  -m, --mode <MODE>          Pattern: pixel | circle | random   [default: pixel]
  -i, --interval <DURATION>  Time between jiggles               [default: 30s]
  -d, --distance <PIXELS>    Movement amplitude in pixels       [default: 1]
      --max-runtime <DUR>    Stop after this total duration     [default: unlimited]
      --once                 Jiggle once and exit
  -q, --quiet                Suppress output
  -v, --verbose              Per-iteration logging
  -h, --help                 Print help
  -V, --version              Print version

DURATION format: integer with optional suffix s (seconds, default), m (minutes), h (hours).
                 e.g. 30s, 5m, 2h, 90 (= 90s)

MODES:
  pixel    Move <distance> pixels right then back. Imperceptible at distance=1.
  circle   Trace a small circle of radius <distance> over 8 steps and return.
  random   Random offset in [-distance,+distance]^2, then return to origin.

EXAMPLES:
  mouse-jiggler                              # default: pixel mode every 30s
  mouse-jiggler -m circle -d 3 -i 1m         # 3px circle every minute
  mouse-jiggler --once -m random -d 5        # one random nudge then exit
  mouse-jiggler --max-runtime 8h             # auto-stop after 8 hours
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pixel,
    Circle,
    Random,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: Mode,
    pub interval: Duration,
    pub distance: i32,
    pub max_runtime: Option<Duration>,
    pub once: bool,
    pub verbose: bool,
    pub quiet: bool,
}

pub enum Action {
    Run(Config),
    Help,
    Version,
}

pub fn parse(args: Vec<String>) -> Result<Action, String> {
    let mut cfg = Config {
        mode: Mode::Pixel,
        interval: Duration::from_secs(30),
        distance: 1,
        max_runtime: None,
        once: false,
        verbose: false,
        quiet: false,
    };

    let mut i = 0;
    while i < args.len() {
        let raw = args[i].clone();
        i += 1;

        let (key, mut inline_val): (String, Option<String>) = match raw.find('=') {
            Some(eq) => (raw[..eq].to_string(), Some(raw[eq + 1..].to_string())),
            None => (raw, None),
        };

        // Capture the next arg (or inline =val) as a value for this option.
        macro_rules! val {
            () => {{
                if let Some(s) = inline_val.take() {
                    s
                } else if i < args.len() {
                    let s = args[i].clone();
                    i += 1;
                    s
                } else {
                    return Err(format!("{} requires a value", key));
                }
            }};
        }

        match key.as_str() {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "-m" | "--mode" => cfg.mode = parse_mode(&val!())?,
            "-i" | "--interval" => cfg.interval = parse_duration(&val!())?,
            "-d" | "--distance" => {
                let v = val!();
                cfg.distance = v
                    .parse()
                    .map_err(|_| format!("--distance: not an integer: {v}"))?;
                if cfg.distance < 1 {
                    return Err("--distance must be >= 1".into());
                }
            }
            "--max-runtime" => cfg.max_runtime = Some(parse_duration(&val!())?),
            "--once" => cfg.once = true,
            "-q" | "--quiet" => cfg.quiet = true,
            "-v" | "--verbose" => cfg.verbose = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    if cfg.quiet && cfg.verbose {
        return Err("--quiet and --verbose are mutually exclusive".into());
    }

    Ok(Action::Run(cfg))
}

fn parse_mode(s: &str) -> Result<Mode, String> {
    match s.to_ascii_lowercase().as_str() {
        "pixel" | "p" => Ok(Mode::Pixel),
        "circle" | "c" => Ok(Mode::Circle),
        "random" | "r" => Ok(Mode::Random),
        other => Err(format!(
            "unknown mode: {other} (valid: pixel, circle, random)"
        )),
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".into());
    }
    let last = *s.as_bytes().last().unwrap();
    let (num_str, mult): (&str, u64) = match last {
        b's' => (&s[..s.len() - 1], 1),
        b'm' => (&s[..s.len() - 1], 60),
        b'h' => (&s[..s.len() - 1], 3600),
        b'0'..=b'9' => (s, 1),
        _ => return Err(format!("bad duration suffix in '{s}' (use s/m/h)")),
    };
    let n: u64 = num_str
        .parse()
        .map_err(|_| format!("bad duration: '{s}'"))?;
    Ok(Duration::from_secs(n.saturating_mul(mult)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg = match parse(vec![]).unwrap() {
            Action::Run(c) => c,
            _ => panic!(),
        };
        assert_eq!(cfg.mode, Mode::Pixel);
        assert_eq!(cfg.interval, Duration::from_secs(30));
        assert_eq!(cfg.distance, 1);
        assert!(!cfg.once);
    }

    #[test]
    fn long_and_short_forms() {
        let cfg = match parse(vec![
            "--mode=circle".into(),
            "-i".into(),
            "5m".into(),
            "-d".into(),
            "4".into(),
            "--once".into(),
        ])
        .unwrap()
        {
            Action::Run(c) => c,
            _ => panic!(),
        };
        assert_eq!(cfg.mode, Mode::Circle);
        assert_eq!(cfg.interval, Duration::from_secs(300));
        assert_eq!(cfg.distance, 4);
        assert!(cfg.once);
    }

    #[test]
    fn duration_suffixes() {
        assert_eq!(parse_duration("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration("2m").unwrap(), Duration::from_secs(120));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert!(parse_duration("3x").is_err());
    }

    #[test]
    fn rejects_unknown() {
        assert!(parse(vec!["--bogus".into()]).is_err());
    }

    #[test]
    fn rejects_zero_distance() {
        assert!(parse(vec!["-d".into(), "0".into()]).is_err());
    }

    #[test]
    fn quiet_and_verbose_conflict() {
        assert!(parse(vec!["-q".into(), "-v".into()]).is_err());
    }
}
