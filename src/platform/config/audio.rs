//! Audio configuration parsing (sample rate, buffering, enable/disable).

use super::FrontendConfig;
use super::cli::{has_negation_flag, parse_bool, parse_bool_arg, parse_u32_arg};

const AUDIO_BUFFER_MIN_MS: u32 = 20;
const AUDIO_BUFFER_MAX_MS: u32 = 500;
const ALLOWED_AUDIO_SAMPLE_RATES: [u32; 5] = [22_050, 44_100, 48_000, 96_000, 192_000];

fn validate_audio_sample_rate(rate: u32) -> Result<u32, String> {
    if ALLOWED_AUDIO_SAMPLE_RATES.contains(&rate) {
        Ok(rate)
    } else {
        Err(format!(
            "Unsupported audio sample rate '{}'. Supported values are: {:?}",
            rate, ALLOWED_AUDIO_SAMPLE_RATES
        ))
    }
}

/// Apply audio command-line flags.
pub(super) fn apply_args(cfg: &mut FrontendConfig, args: &[String]) -> Result<(), String> {
    // Audio: --audio true/false, --no-audio, --disable-audio
    if let Some(audio) = parse_bool_arg(args, "--audio")? {
        cfg.audio_enabled = audio;
    }
    if has_negation_flag(args, &["--no-audio", "--disable-audio"]) {
        cfg.audio_enabled = false;
    }

    if let Some(buffer_ms) = parse_u32_arg(args, "--audio-buffer-ms")? {
        cfg.audio_buffer_ms = buffer_ms.clamp(AUDIO_BUFFER_MIN_MS, AUDIO_BUFFER_MAX_MS);
    }

    if let Some(rate) = parse_u32_arg(args, "--audio-sample-rate")? {
        cfg.audio_sample_rate =
            validate_audio_sample_rate(rate).map_err(|e| format!("--audio-sample-rate: {e}"))?;
    }
    Ok(())
}

/// Apply an audio config-file key/value. Returns `true` if the key was handled.
pub(super) fn apply_config_value(
    cfg: &mut FrontendConfig,
    key: &str,
    value: &str,
) -> Result<bool, String> {
    match key {
        "audio" => {
            if let Ok(b) = parse_bool(value) {
                cfg.audio_enabled = b;
            }
        }
        "audio_buffer_ms" => {
            if let Ok(ms) = value.parse::<u32>() {
                cfg.audio_buffer_ms = ms.clamp(AUDIO_BUFFER_MIN_MS, AUDIO_BUFFER_MAX_MS);
            }
        }
        "audio_sample_rate" => match value.parse::<u32>() {
            Ok(rate) => {
                cfg.audio_sample_rate = validate_audio_sample_rate(rate)?;
            }
            Err(_) => {
                return Err(format!(
                    "Invalid value for audio_sample_rate '{}'. Expected a positive integer",
                    value
                ));
            }
        },
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{AUDIO_BUFFER_MAX_MS, AUDIO_BUFFER_MIN_MS};
    use crate::nes::console::Config;
    use crate::platform::config::FrontendConfig;
    use crate::platform::config::test_support::parse_config;

    #[test]
    fn test_config_audio_false() {
        let args = vec![
            "neser".to_string(),
            "--audio".to_string(),
            "false".to_string(),
        ];
        let config = parse_config(args);
        assert!(!config.frontend.audio_enabled);
    }

    #[test]
    fn test_config_audio_buffer_ms_default() {
        let args = vec!["neser".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.audio_buffer_ms, 60);
    }

    #[test]
    fn test_config_audio_buffer_ms_from_cli() {
        let args = vec![
            "neser".to_string(),
            "--audio-buffer-ms".to_string(),
            "75".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.audio_buffer_ms, 75);
    }

    #[test]
    fn test_config_audio_buffer_ms_clamps_from_config_value() {
        let mut config = FrontendConfig::default();

        config
            .apply_config_value("audio_buffer_ms", "5")
            .expect("config value should parse");
        assert_eq!(config.audio_buffer_ms, AUDIO_BUFFER_MIN_MS);

        config
            .apply_config_value("audio_buffer_ms", "5000")
            .expect("config value should parse");
        assert_eq!(config.audio_buffer_ms, AUDIO_BUFFER_MAX_MS);
    }

    #[test]
    fn test_config_audio_sample_rate_default() {
        let args = vec!["neser".to_string()];
        let config = parse_config(args);
        assert_eq!(config.frontend.audio_sample_rate, 44100);
    }

    #[test]
    fn test_config_audio_sample_rate_from_cli() {
        let args = vec![
            "neser".to_string(),
            "--audio-sample-rate".to_string(),
            "48000".to_string(),
        ];
        let config = parse_config(args);
        assert_eq!(config.frontend.audio_sample_rate, 48000);
    }

    #[test]
    fn test_config_audio_sample_rate_from_config_value() {
        let mut config = FrontendConfig::default();

        config
            .apply_config_value("audio_sample_rate", "96000")
            .expect("config value should parse");
        assert_eq!(config.audio_sample_rate, 96000);
    }

    #[test]
    fn test_config_audio_sample_rate_rejects_unsupported_rates() {
        let mut config = FrontendConfig::default();

        assert!(
            config
                .apply_config_value("audio_sample_rate", "12345")
                .is_err()
        );
        assert!(config.apply_config_value("audio-sample-rate", "0").is_err());
        assert!(
            config
                .apply_config_value("audio_sample_rate", "-44100")
                .is_err()
        );
        assert!(
            config
                .apply_config_value("audio_sample_rate", "not_a_number")
                .is_err()
        );
    }

    #[test]
    fn test_config_audio_sample_rate_cli_rejects_invalid_values() {
        let args = vec![
            "neser".to_string(),
            "--audio-sample-rate".to_string(),
            "12345".to_string(),
        ];
        assert!(Config::new(&args).is_err());
    }
}
