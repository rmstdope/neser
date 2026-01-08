#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tracing {
    pub enabled: bool,
    pub ppu: bool,
    pub apu: bool,
    pub nestest: bool,
}

impl Tracing {
    pub fn from_args(args: &[String]) -> Self {
        let mut tracing = Tracing::default();

        for arg in args {
            if arg == "--trace" {
                tracing.enabled = true;
                continue;
            }

            if arg.starts_with("--trace-") {
                tracing.enabled = true;
                match arg.as_str() {
                    "--trace-nestest" => tracing.nestest = true,
                    "--trace-ppu" => tracing.ppu = true,
                    "--trace-apu" => tracing.apu = true,
                    _ => {}
                }
            }
        }

        tracing
    }
}

pub fn parse_tracing_from_args(args: &[String]) -> Tracing {
    Tracing::from_args(args)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tracing_defaults_to_disabled() {
        let args = vec!["neser".to_string()];
        let tracing = super::parse_tracing_from_args(&args);
        assert!(!tracing.enabled);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_is_enabled_with_trace_flag() {
        let args = vec!["neser".to_string(), "--trace".to_string()];
        let tracing = super::parse_tracing_from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_uses_nestest_format_when_requested() {
        let args = vec!["neser".to_string(), "--trace-nestest".to_string()];
        let tracing = super::parse_tracing_from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.ppu);
        assert!(!tracing.apu);
        assert!(tracing.nestest);
    }

    #[test]
    fn tracing_enables_ppu_trace_with_trace_ppu_flag() {
        let args = vec!["neser".to_string(), "--trace-ppu".to_string()];
        let tracing = super::parse_tracing_from_args(&args);
        assert!(tracing.enabled);
        assert!(tracing.ppu);
        assert!(!tracing.apu);
        assert!(!tracing.nestest);
    }

    #[test]
    fn tracing_enables_apu_trace_with_trace_apu_flag() {
        let args = vec!["neser".to_string(), "--trace-apu".to_string()];
        let tracing = super::parse_tracing_from_args(&args);
        assert!(tracing.enabled);
        assert!(!tracing.ppu);
        assert!(tracing.apu);
        assert!(!tracing.nestest);
    }
}
