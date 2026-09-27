//! How far the host mouse moves the SNES Mouse: moving across the picture as it is drawn
//! moves the game's pointer across the game screen, at any window size. The SNES Mouse
//! reports counts that games treat as picture pixels (at the slowest sensitivity), so a
//! host movement is scaled by the picture's on-screen size. Fractions carry over to the
//! next movement, so a slow hand in a large window still moves the pointer.

/// The game screen's width in pixels.
const GAME_WIDTH: f32 = 256.0;
/// The game screen's height in lines.
const GAME_HEIGHT: f32 = 224.0;

/// Scales host pointer movement to SNES Mouse counts, carrying fractions between calls.
/// One per mouse capture surface (the desktop window, the web page); fresh per game.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MouseMotionScale {
    carry_x: f32,
    carry_y: f32,
}

impl MouseMotionScale {
    /// The counts for a host movement of (`dx`, `dy`) over a picture drawn
    /// `picture_width` × `picture_height` large, in the same units as the movement.
    pub fn counts(
        &mut self,
        dx: f32,
        dy: f32,
        picture_width: f32,
        picture_height: f32,
    ) -> (i16, i16) {
        let drawn = |extent: f32| extent.is_finite() && extent > 0.0;
        if !drawn(picture_width) || !drawn(picture_height) {
            return (0, 0);
        }
        (
            Self::take_whole(&mut self.carry_x, dx * GAME_WIDTH / picture_width),
            Self::take_whole(&mut self.carry_y, dy * GAME_HEIGHT / picture_height),
        )
    }

    /// Adds `delta` to `carry` and takes out the whole counts, leaving the fraction behind.
    fn take_whole(carry: &mut f32, delta: f32) -> i16 {
        if !delta.is_finite() {
            return 0;
        }
        let total = *carry + delta;
        let whole = total
            .trunc()
            .clamp(f32::from(i16::MIN), f32::from(i16::MAX));
        *carry = total - total.trunc();
        whole as i16
    }
}

#[cfg(test)]
mod tests {
    use super::MouseMotionScale;

    #[test]
    fn crossing_the_picture_crosses_the_game_screen() {
        let mut scale = MouseMotionScale::default();
        assert_eq!(scale.counts(512.0, 448.0, 512.0, 448.0), (256, 224));
    }

    #[test]
    fn same_hand_motion_same_counts_at_any_size() {
        // A quarter of the picture is a quarter of the screen, small window or full screen.
        let mut small = MouseMotionScale::default();
        let mut large = MouseMotionScale::default();
        assert_eq!(small.counts(64.0, -56.0, 256.0, 224.0), (64, -56));
        assert_eq!(large.counts(480.0, -270.0, 1920.0, 1080.0), (64, -56));
    }

    #[test]
    fn slow_motion_accumulates_instead_of_vanishing() {
        // In a picture four times the game's size, each one-point step is a quarter count.
        let mut scale = MouseMotionScale::default();
        let steps: Vec<(i16, i16)> = (0..4)
            .map(|_| scale.counts(1.0, -1.0, 1024.0, 896.0))
            .collect();
        assert_eq!(steps, [(0, 0), (0, 0), (0, 0), (1, -1)]);
    }

    #[test]
    fn degenerate_picture_moves_nothing() {
        let mut scale = MouseMotionScale::default();
        assert_eq!(scale.counts(10.0, 10.0, 0.0, 224.0), (0, 0));
        assert_eq!(scale.counts(10.0, 10.0, 256.0, f32::NAN), (0, 0));
    }
}
