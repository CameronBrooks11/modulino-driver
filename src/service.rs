//! Address change and general-call reset: the only code that sends a
//! firmware command.
//!
//! A node acts on a frame that starts `CF` once it has received exactly its
//! write length (main.c:531-561): it stores the third byte in flash as its
//! 8-bit address, then resets (main.c:238-253). At boot it takes the stored
//! byte as its address unless that byte is 0x00 or 0xFF or fails its check
//! byte, and falls back to its strap otherwise (main.c:160-167). So `CF`
//! with 0x00 restores the strap default. The board answers again once it has
//! rebooted; Arduino's AddressChanger waits 500 ms (AddressChanger.ino:151).
//!
//! These functions work on a [`Node`]: to change a device's address, release
//! its bus, wrap it in a `Node` of the device's kind, and build the device
//! again afterwards.
//!
//! ```no_run
//! # use embedded_hal_mock::eh1::{delay::NoopDelay, i2c::Mock};
//! # fn main() -> Result<(), modulino_driver::Error<embedded_hal::i2c::ErrorKind>> {
//! # let i2c = Mock::new(&[]);
//! # let mut delay = NoopDelay::new();
//! use modulino_driver::{service, Kind, NewAddress, Node};
//!
//! let mut node = Node::new(i2c, Kind::Buttons.default_address(), Kind::Buttons);
//! service::set_address(&mut node, NewAddress::new(0x40).unwrap(), &mut delay)?;
//! assert_eq!(node.address().get(), 0x40);
//! # Ok(())
//! # }
//! ```

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

use crate::addr::{Address, NewAddress};
use crate::chips::acked;
use crate::codec::led_matrix::{decode_mode, Mode, READ_PAYLOAD as MATRIX_PAYLOAD};
use crate::node::Node;
use crate::scan::silent_chip;
use crate::spec::{WriteLen, NODES};
use crate::Error;

/// How long a board takes to store its address and reboot, in ms: what
/// Arduino's AddressChanger waits (AddressChanger.ino:151).
pub const RESET_MS: u32 = 500;

/// The general-call address.
const GENERAL_CALL: u8 = 0x00;

/// The longest write any node accepts.
const MAX_WRITE: usize = max_write_len();

const fn max_write_len() -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < NODES.len() {
        let len = match NODES[i].write_len {
            WriteLen::Fixed(n) => n,
            WriteLen::Mode { mono, grayscale } => {
                if mono > grayscale {
                    mono
                } else {
                    grayscale
                }
            }
        };
        if len > max {
            max = len;
        }
        i += 1;
    }
    max
}

/// Move the node to `new`, and point `node` at it once its strap is
/// confirmed there.
///
/// The board is identified first, and nothing is sent if `new` is already
/// its address. If something answers at `new`, the result is
/// [`Error::AddressInUse`] and nothing is sent. The check is a 1-byte read;
/// at 0x45, where a Modulino Thermo's SHT31 sits and NACKs reads while
/// idle, a NACKed read is followed by the SHT3x status command, so a device
/// there that NACKs reads receives that write. The `CF` frame is padded to the board's current write
/// length (48 bytes for a grayscale LED matrix), then the board gets
/// [`RESET_MS`] to reboot.
///
/// The check covers the target, not the source: two boards of one kind at
/// the same address (two new boards of a kind start at the same default)
/// both take the command and both move. Connect one board per address when
/// changing addresses.
///
/// On an error after the command was sent, `node` keeps its old address;
/// the board may be at either.
pub fn set_address<I2C: I2c>(
    node: &mut Node<I2C>,
    new: NewAddress,
    delay: &mut impl DelayNs,
) -> Result<(), Error<I2C::Error>> {
    move_to(node, Address::from(new), new.get() << 1, delay)
}

/// Return the node to its strap default address, the one its board answers
/// at out of the box, with `CF 0x00`.
///
/// Checks as [`set_address`] does: nothing is sent if the board is already
/// there, and [`Error::AddressInUse`] if something answers at the default.
pub fn reset_address<I2C: I2c>(
    node: &mut Node<I2C>,
    delay: &mut impl DelayNs,
) -> Result<(), Error<I2C::Error>> {
    // A strap of another kind is refused by `move_to`'s verify; one that
    // matches no kind has no default.
    let default = node.identify()?.default_address();
    match default {
        Some(default) => move_to(node, default, 0x00, delay),
        None => node.verify(),
    }
}

/// Send `CF 0x00` as a general call, which returns every node on the bus to
/// its strap default address, then wait [`RESET_MS`].
///
/// Each kind acts only on a frame of its own write length, so the command
/// goes out once per length, shortest first. A board takes the first frame
/// of its own length before it has seen any longer frame. It acts on the
/// command from its main loop, after erasing a flash page
/// (main.c:219-221, 238-253), so it may still be listening when the longer
/// frames arrive. Those carry the same command, but how a board handles a
/// frame longer than its length is unverified (design section 7). Boards
/// already at their default are reset too.
///
/// "The bus" is the segment the call goes out on: through a [`HubPort`],
/// that is the selected port and the upstream bus it is joined to; on a
/// bus with a hub, boards behind ports that are not selected are not
/// reached. Nothing is checked first, so boards that share a default end up
/// sharing an address.
///
/// Every length is sent even if one fails. After the wait, returns the
/// first bus error other than a NACK if there was one, even if other frames
/// were acknowledged; otherwise whether any frame was acknowledged. A NACK is not an error, since a frame no board takes may be
/// refused.
///
/// [`HubPort`]: crate::HubPort
pub fn general_call_reset<I2C: I2c>(
    bus: &mut I2C,
    delay: &mut impl DelayNs,
) -> Result<bool, Error<I2C::Error>> {
    let mut lengths = [0usize; 2 * NODES.len()];
    let mut count = 0;
    for spec in NODES.iter() {
        let (a, b) = match spec.write_len {
            WriteLen::Fixed(n) => (n, n),
            WriteLen::Mode { mono, grayscale } => (mono, grayscale),
        };
        for len in [a, b] {
            if !lengths[..count].contains(&len) {
                lengths[count] = len;
                count += 1;
            }
        }
    }
    lengths[..count].sort_unstable();

    // Boards that took an earlier frame are resetting whatever happens to
    // the later ones, so every length is sent and the wait always runs.
    let frame = command(0x00);
    let mut any = false;
    let mut first_error = None;
    for &len in &lengths[..count] {
        match acked(bus.write(GENERAL_CALL, &frame[..len])) {
            Ok(acknowledged) => any |= acknowledged,
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    delay.delay_ms(RESET_MS);
    match first_error {
        Some(e) => Err(e),
        None => Ok(any),
    }
}

/// Send `CF byte` to the node and confirm it at `target`.
fn move_to<I2C: I2c>(
    node: &mut Node<I2C>,
    target: Address,
    byte: u8,
    delay: &mut impl DelayNs,
) -> Result<(), Error<I2C::Error>> {
    node.verify()?;
    if node.address() == target {
        return Ok(());
    }
    let len = write_len(node)?;
    if occupied(node.bus(), target)? {
        return Err(Error::AddressInUse);
    }
    node.write_command(&command(byte)[..len])?;
    delay.delay_ms(RESET_MS);

    let old = node.address();
    node.set_addr(target);
    if let Err(e) = node.verify() {
        node.set_addr(old);
        return Err(e);
    }
    Ok(())
}

/// Whether anything answers at `addr`: a 1-byte read, or, at an address
/// where scan expects an SHT3x (0x45, the Modulino Thermo's), its status
/// command, since it NACKs reads while idle.
fn occupied<I2C: I2c>(bus: &mut I2C, addr: Address) -> Result<bool, Error<I2C::Error>> {
    let mut probe = [0u8; 1];
    if acked(bus.read(addr.get(), &mut probe))? {
        return Ok(true);
    }
    Ok(silent_chip(bus, addr.get())?.is_some())
}

/// The write length the board takes now. For the LED matrix, the only kind
/// whose length follows a mode, the mode is read from the board.
fn write_len<I2C: I2c>(node: &mut Node<I2C>) -> Result<usize, Error<I2C::Error>> {
    match node.kind().spec().write_len {
        WriteLen::Fixed(n) => Ok(n),
        WriteLen::Mode { mono, grayscale } => match decode_mode(&node.read::<MATRIX_PAYLOAD>()?) {
            Some(Mode::Mono) => Ok(mono),
            Some(Mode::Grayscale) => Ok(grayscale),
            None => Err(Error::DataError),
        },
    }
}

/// `CF byte`, padded with zeros to the longest write.
fn command(byte: u8) -> [u8; MAX_WRITE] {
    let mut frame = [0u8; MAX_WRITE];
    frame[..3].copy_from_slice(&[b'C', b'F', byte]);
    frame
}
