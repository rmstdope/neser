use crate::snes::apu::dsp::voice::{EnvelopeMode, VoiceState};

const ENV_MAX: u16 = 0x7FF;

pub fn step_voice_envelope(voice: &mut VoiceState) {
    if !envelope_tick_due(voice) {
        return;
    }

    if voice.adsr1 & 0x80 != 0 {
        step_adsr(voice);
    } else {
        step_gain(voice);
    }

    voice.env_level = voice.env_level.min(ENV_MAX);
    voice.envx = (voice.env_level >> 4).min(0x7F) as u8;
}

fn envelope_tick_due(voice: &mut VoiceState) -> bool {
    let rate = active_rate(voice);
    let divider = rate_to_divider(rate);
    voice.env_divider_counter = voice.env_divider_counter.wrapping_add(1);
    if voice.env_divider_counter < divider {
        return false;
    }
    voice.env_divider_counter = 0;
    true
}

fn active_rate(voice: &VoiceState) -> u8 {
    if voice.adsr1 & 0x80 != 0 {
        match voice.mode {
            EnvelopeMode::Attack => voice.adsr1 & 0x0F,
            EnvelopeMode::Decay => ((voice.adsr1 >> 4) & 0x07) * 2 + 1,
            EnvelopeMode::Sustain => voice.adsr2 & 0x1F,
            EnvelopeMode::Release => 31,
        }
    } else {
        voice.gain & 0x1F
    }
}

fn rate_to_divider(rate: u8) -> u16 {
    const RATE_TO_DIV: [u16; 32] = [
        2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 1, 48, 40, 32,
        24, 20, 16, 12, 10, 8, 6, 5, 4, 3, 2, 1, 1,
    ];
    RATE_TO_DIV[usize::from(rate.min(31))]
}

fn step_adsr(voice: &mut VoiceState) {
    match voice.mode {
        EnvelopeMode::Release => {
            voice.env_level = voice.env_level.saturating_sub(8);
        }
        EnvelopeMode::Attack => {
            let attack_rate = voice.adsr1 & 0x0F;
            let attack_step = if attack_rate == 0x0F { 0x20 } else { 0x08 };
            voice.env_level = voice.env_level.saturating_add(attack_step);
            if voice.env_level >= ENV_MAX {
                voice.env_level = ENV_MAX;
                voice.mode = EnvelopeMode::Decay;
            }
        }
        EnvelopeMode::Decay => {
            let sustain_level = (((voice.adsr2 >> 5) & 0x07) + 1) as u16 * 0x100;
            let decay_step = ((voice.env_level.saturating_sub(1)) >> 8) + 1;
            voice.env_level = voice.env_level.saturating_sub(decay_step);
            if voice.env_level <= sustain_level {
                voice.mode = EnvelopeMode::Sustain;
            }
        }
        EnvelopeMode::Sustain => {
            let sustain_step = ((voice.env_level.saturating_sub(1)) >> 8) + 1;
            voice.env_level = voice.env_level.saturating_sub(sustain_step);
        }
    }
}

fn step_gain(voice: &mut VoiceState) {
    let gain = voice.gain;
    if gain & 0x80 == 0 {
        voice.env_level = u16::from(gain) << 4;
        return;
    }

    match (gain >> 5) & 0x03 {
        0 => {
            voice.env_level = voice.env_level.saturating_sub(0x20);
        }
        1 => {
            let step = ((voice.env_level.saturating_sub(1)) >> 8) + 1;
            voice.env_level = voice.env_level.saturating_sub(step);
        }
        2 => {
            voice.env_level = (voice.env_level + 0x20).min(ENV_MAX);
        }
        _ => {
            let step = if voice.env_level < 0x600 { 0x20 } else { 0x08 };
            voice.env_level = (voice.env_level + step).min(ENV_MAX);
        }
    }
}
