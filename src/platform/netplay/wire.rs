//! The packet two lockstep sessions exchange, and its byte encoding.
//!
//! Everything a peer sends is untrusted: [`Packet::decode`] returns an error for any input
//! it does not accept and never panics.
//!
//! Layout, little-endian:
//!
//! ```text
//! version u8 | ack u32 | first_frame u32 | count u8 | count × u16 | has_hash u8 | [frame u32 | hash u32]
//! ```

/// The version byte every packet starts with; a packet with another is refused.
pub const WIRE_VERSION: u8 = 1;
/// The most inputs one packet carries (the count is one byte).
pub const MAX_INPUTS_PER_PACKET: usize = 255;

/// What one side tells the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// The first of the sender's frames the sender still needs from the receiver.
    pub ack: u32,
    /// The frame `inputs[0]` belongs to.
    pub first_frame: u32,
    /// The sender's pad, one bit per button id, for `first_frame`, `first_frame + 1`, ...
    pub inputs: Vec<u16>,
    /// The sender's newest state hash, as `(frame, hash)`.
    pub state_hash: Option<(u32, u32)>,
}

/// Why bytes from a peer were not accepted as a [`Packet`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// The bytes end before the packet does.
    Truncated,
    /// The packet starts with a version this build does not speak.
    UnknownVersion(u8),
    /// The hash flag is neither 0 nor 1.
    BadHashFlag(u8),
    /// Bytes follow the end of the packet.
    TrailingBytes,
}

impl Packet {
    /// The packet as bytes.
    ///
    /// # Panics
    ///
    /// If `inputs` holds more than [`MAX_INPUTS_PER_PACKET`]; [`super::lockstep::Lockstep`]
    /// never builds such a packet.
    pub fn encode(&self) -> Vec<u8> {
        let count = u8::try_from(self.inputs.len()).unwrap_or_else(|_| {
            panic!(
                "{} inputs do not fit one packet (at most {MAX_INPUTS_PER_PACKET})",
                self.inputs.len()
            )
        });
        let mut bytes = Vec::with_capacity(11 + 2 * self.inputs.len() + 9);
        bytes.push(WIRE_VERSION);
        bytes.extend_from_slice(&self.ack.to_le_bytes());
        bytes.extend_from_slice(&self.first_frame.to_le_bytes());
        bytes.push(count);
        for input in &self.inputs {
            bytes.extend_from_slice(&input.to_le_bytes());
        }
        match self.state_hash {
            None => bytes.push(0),
            Some((frame, hash)) => {
                bytes.push(1);
                bytes.extend_from_slice(&frame.to_le_bytes());
                bytes.extend_from_slice(&hash.to_le_bytes());
            }
        }
        bytes
    }

    /// The packet in `bytes`, or why they are not one.
    pub fn decode(bytes: &[u8]) -> Result<Packet, WireError> {
        let mut reader = Reader(bytes);
        let version = reader.u8()?;
        if version != WIRE_VERSION {
            return Err(WireError::UnknownVersion(version));
        }
        let ack = reader.u32()?;
        let first_frame = reader.u32()?;
        let count = reader.u8()?;
        let inputs = (0..count)
            .map(|_| reader.u16())
            .collect::<Result<Vec<_>, _>>()?;
        let state_hash = match reader.u8()? {
            0 => None,
            1 => Some((reader.u32()?, reader.u32()?)),
            flag => return Err(WireError::BadHashFlag(flag)),
        };
        if !reader.0.is_empty() {
            return Err(WireError::TrailingBytes);
        }
        Ok(Packet {
            ack,
            first_frame,
            inputs,
            state_hash,
        })
    }
}

/// Reads little-endian fields off the front of a byte slice.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(WireError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Packet {
        Packet {
            ack: 0x0102_0304,
            first_frame: 77,
            inputs: vec![0x0000, 0x0FFF, 0x8001],
            state_hash: Some((60, 0xDEAD_BEEF)),
        }
    }

    #[test]
    fn packet_round_trips_through_bytes() {
        for packet in [
            sample(),
            Packet {
                state_hash: None,
                inputs: vec![],
                ..sample()
            },
            Packet {
                inputs: vec![0xABCD; MAX_INPUTS_PER_PACKET],
                ..sample()
            },
        ] {
            assert_eq!(Packet::decode(&packet.encode()), Ok(packet));
        }
    }

    #[test]
    fn layout_is_little_endian_as_documented() {
        let bytes = Packet {
            ack: 1,
            first_frame: 2,
            inputs: vec![0x0304],
            state_hash: None,
        }
        .encode();
        assert_eq!(
            bytes,
            [WIRE_VERSION, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0x04, 0x03, 0]
        );
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let bytes = sample().encode();
        for len in 0..bytes.len() {
            assert_eq!(
                Packet::decode(&bytes[..len]),
                Err(WireError::Truncated),
                "prefix of {len} bytes"
            );
        }
    }

    #[test]
    fn unknown_version_is_rejected() {
        let mut bytes = sample().encode();
        bytes[0] = WIRE_VERSION + 1;
        assert_eq!(
            Packet::decode(&bytes),
            Err(WireError::UnknownVersion(WIRE_VERSION + 1))
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = sample().encode();
        bytes.push(0);
        assert_eq!(Packet::decode(&bytes), Err(WireError::TrailingBytes));
    }

    #[test]
    fn a_hash_flag_other_than_zero_or_one_is_rejected() {
        let mut bytes = Packet {
            state_hash: None,
            ..sample()
        }
        .encode();
        *bytes.last_mut().unwrap() = 2;
        assert_eq!(Packet::decode(&bytes), Err(WireError::BadHashFlag(2)));
    }

    #[test]
    #[should_panic(expected = "inputs")]
    fn encoding_more_than_the_maximum_inputs_panics() {
        Packet {
            inputs: vec![0; MAX_INPUTS_PER_PACKET + 1],
            ..sample()
        }
        .encode();
    }
}
