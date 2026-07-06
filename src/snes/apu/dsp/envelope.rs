use crate::snes::apu::dsp::voice::{EnvelopeMode, VoiceState};

const ENV_MAX: u16 = 0x7FF;

pub fn step_voice_envelope(voice: &mut VoiceState, global_counter: u16) {
    if voice.mode == EnvelopeMode::Release {
        voice.env_level = voice.env_level.saturating_sub(8);
        return;
    }

    let (next_env, raw_env, rate) = if voice.adsr1_latch & 0x80 != 0 {
        next_adsr_env(voice)
    } else {
        next_gain_env(voice)
    };

    apply_state_transitions(voice, raw_env);
    voice.hidden_env = raw_env;

    if envelope_tick_due(global_counter, rate) {
        voice.env_level = next_env;
    }
}

fn apply_state_transitions(voice: &mut VoiceState, raw_env: i32) {
    if voice.mode == EnvelopeMode::Decay && (raw_env >> 8) == i32::from(transition_data(voice) >> 5)
    {
        voice.mode = EnvelopeMode::Sustain;
    }

    if !(0..=i32::from(ENV_MAX)).contains(&raw_env) && voice.mode == EnvelopeMode::Attack {
        voice.mode = EnvelopeMode::Decay;
    }
}

fn transition_data(voice: &VoiceState) -> u8 {
    if voice.adsr1_latch & 0x80 != 0 {
        voice.adsr2
    } else {
        voice.gain
    }
}

pub(super) fn envelope_tick_due(global_counter: u16, rate: u8) -> bool {
    let rate = usize::from(rate.min(31));
    let divider = COUNTER_RATES[rate];
    (u32::from(global_counter) + COUNTER_OFFSETS[rate]).is_multiple_of(divider)
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

fn next_adsr_env(voice: &VoiceState) -> (u16, i32, u8) {
    let mut env = i32::from(voice.env_level);
    let rate = match voice.mode {
        EnvelopeMode::Attack => {
            let rate = (voice.adsr1_latch & 0x0F) * 2 + 1;
            env += if rate < 31 { 0x20 } else { 0x400 };
            rate
        }
        EnvelopeMode::Decay => {
            env -= 1;
            env -= env >> 8;
            ((voice.adsr1_latch >> 3) & 0x0E) + 0x10
        }
        EnvelopeMode::Sustain => {
            env -= 1;
            env -= env >> 8;
            voice.adsr2 & 0x1F
        }
        EnvelopeMode::Release => 31,
    };
    (clamp_env(env), env, rate)
}

fn next_gain_env(voice: &VoiceState) -> (u16, i32, u8) {
    let gain = voice.gain;
    if gain & 0x80 == 0 {
        let env = i32::from(gain) * 0x10;
        return (clamp_env(env), env, 31);
    }

    let mut env = i32::from(voice.env_level);
    let rate = gain & 0x1F;
    match (gain >> 5) & 0x03 {
        0 => {
            env -= 0x20;
        }
        1 => {
            env -= 1;
            env -= env >> 8;
        }
        2 => {
            env += 0x20;
        }
        _ => {
            let step = if (voice.hidden_env as u32) < 0x600 {
                0x20
            } else {
                0x08
            };
            env += step;
        }
    }
    (clamp_env(env), env, rate)
}

fn clamp_env(env: i32) -> u16 {
    env.clamp(0, i32::from(ENV_MAX)) as u16
}
