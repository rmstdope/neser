/**
 * Clamp a GB APU sample to the valid bipolar range [-1.0, 1.0].
 *
 * The GB APU `mix()` function produces bipolar f32 samples in [-1.0, 1.0].
 * Web Audio expects PCM data in the same range, so we just apply a safety
 * clamp without any rescaling.
 */
export function normalizeGbSample(sample: number): number {
    return Math.min(1.0, Math.max(-1.0, sample));
}

/**
 * Normalize a NES APU sample from its native unipolar range [0, nesApuMax]
 * to [0.0, 1.0] for Web Audio.
 *
 * The NES APU pulse+TND mixer outputs up to ~0.966; expansion audio from
 * mappers such as VRC6, MMC5, or Namco 163 can push this higher.  A
 * conservative cap of 1.177 is typically used.
 */
export function normalizeNesSample(sample: number, nesApuMax: number): number {
    if (!Number.isFinite(nesApuMax) || nesApuMax <= 0.0) {
        return 0.0;
    }
    const normalized = sample / nesApuMax;
    return Math.min(1.0, Math.max(0.0, normalized));
}
