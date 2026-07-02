use crate::snes::apu::dsp::voice::{EnvelopeMode, VoiceState};

const ENV_MAX: u16 = 0x7FF;

pub fn step_voice_envelope(voice: &mut VoiceState, global_counter: u16) {
    if voice.mode == EnvelopeMode::Release {
        if envelope_tick_due(global_counter, 31) {
            voice.env_level = voice.env_level.saturating_sub(8);
            voice.envx = ((voice.env_level >> 4).min(0x7F)) as u8;
        }
        return;
    }

    if voice.adsr1 & 0x80 == 0 && voice.gain & 0x80 == 0 {
        voice.env_level = (u16::from(voice.gain) << 4).min(ENV_MAX);
        voice.envx = (voice.env_level >> 4).min(0x7F) as u8;
        apply_gain_state_transitions(voice);
        return;
    }

    if !envelope_tick_due(global_counter, active_rate(voice)) {
        return;
    }

    if voice.adsr1 & 0x80 != 0 {
        step_adsr(voice);
    } else {
        step_gain(voice);
    }

    voice.env_level = voice.env_level.min(ENV_MAX);
    if voice.adsr1 & 0x80 == 0 {
        apply_gain_state_transitions(voice);
    }
    voice.envx = ((voice.env_level >> 4).min(0x7F)) as u8;
}

fn apply_gain_state_transitions(voice: &mut VoiceState) {
    let gain_mode = (voice.gain >> 5) & 0x03;
    let saturated_gain_increase =
        voice.env_level == ENV_MAX && voice.gain & 0x80 != 0 && gain_mode >= 2;
    if voice.mode == EnvelopeMode::Decay
        && !saturated_gain_increase
        && (voice.env_level >> 8) == u16::from(voice.gain >> 5)
    {
        voice.mode = EnvelopeMode::Sustain;
    } else if voice.mode == EnvelopeMode::Attack && voice.env_level == ENV_MAX {
        voice.mode = EnvelopeMode::Decay;
    }
}

fn envelope_tick_due(global_counter: u16, rate: u8) -> bool {
    let rate = usize::from(rate.min(31));
    let divider = COUNTER_RATES[rate];
    (u32::from(global_counter) + COUNTER_OFFSETS[rate]).is_multiple_of(divider)
}

fn active_rate(voice: &VoiceState) -> u8 {
    if voice.adsr1 & 0x80 != 0 {
        match voice.mode {
            EnvelopeMode::Attack => (voice.adsr1 & 0x0F) * 2 + 1,
            EnvelopeMode::Decay => ((voice.adsr1 >> 4) & 0x07) * 2 + 0x10,
            EnvelopeMode::Sustain => voice.adsr2 & 0x1F,
            EnvelopeMode::Release => 31,
        }
    } else {
        voice.gain & 0x1F
    }
}

const SIMPLE_COUNTER_RANGE: u32 = 30_720;
const COUNTER_RATES: [u32; 32] = [
    SIMPLE_COUNTER_RANGE + 1,
    2048,
    1536,
    1280,
    1024,
    768,
    640,
    512,
    384,
    320,
    256,
    192,
    160,
    128,
    96,
    80,
    64,
    48,
    40,
    32,
    24,
    20,
    16,
    12,
    10,
    8,
    6,
    5,
    4,
    3,
    2,
    1,
];
const COUNTER_OFFSETS: [u32; 32] = [
    1, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040,
    536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 0, 0,
];

fn step_adsr(voice: &mut VoiceState) {
    match voice.mode {
        EnvelopeMode::Release => {
            voice.env_level = voice.env_level.saturating_sub(8);
        }
        EnvelopeMode::Attack => {
            let attack_rate = voice.adsr1 & 0x0F;
            let attack_step = if attack_rate == 0x0F { 0x400 } else { 0x20 };
            voice.env_level = voice.env_level.saturating_add(attack_step);
            if voice.env_level >= 0x7E0 {
                voice.env_level = voice.env_level.min(ENV_MAX);
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
