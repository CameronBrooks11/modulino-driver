//! Bench checks on real boards from a Linux host (a Raspberry Pi), one
//! subcommand per module. docs/bench.md is the checklist that runs them.
//!
//! ```sh
//! cargo run --example bench -- <command> [address]
//! ```
//!
//! The bus is `/dev/i2c-1`, or `MODULINO_I2C` if set. Each command prints
//! what to look for (`expect:`) and stops by itself; the interactive ones run
//! for 10 s. None of them changes a board's address or sends a firmware
//! command.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("the bench example needs Linux I2C device files (/dev/i2c-*)");
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    bench::main()
}

#[cfg(target_os = "linux")]
mod bench {
    use std::cell::RefCell;
    use std::process::ExitCode;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    use embedded_hal::i2c::{self, ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};
    use linux_embedded_hal::i2cdev::linux::LinuxI2CError;
    use linux_embedded_hal::{Delay, I2CError, I2cdev};
    use modulino_driver::{
        scan, scan_except, Address, Buttons, Buzzer, Color, Distance, Error, Found, Hub, Joystick,
        Knob, LatchRelay, LedMatrix, Light, MatrixMode, MotorMode, Motors, Movement, Node, Note,
        OptoRelay, Pixels, PowerLevel, Pressure, Thermo, Vibro,
    };

    /// `EREMOTEIO` in Linux's asm-generic errno.h, which arm and arm64 use.
    const EREMOTEIO: i32 = 121;

    const COMMANDS: &str = "frames nack scan hub buttons buzzer vibro knob pixels joystick \
         opto-relay latch-relay led-matrix motors motors-stepper thermo distance \
         movement pressure light";

    /// The host bus, with one change: an `EREMOTEIO` is reported as a NACK.
    ///
    /// The Pi's I2C driver (i2c-bcm2835) returns `EREMOTEIO` when a transfer
    /// is not acknowledged; linux-embedded-hal maps only `ENXIO` and `ENODEV`
    /// to `NoAcknowledge` and reports `EREMOTEIO` as `Other`. The `nack`
    /// command shows what the host returns without this change.
    pub struct Bus(I2cdev);

    #[derive(Debug)]
    pub struct BusError(I2CError);

    fn errno(e: &I2CError) -> Option<i32> {
        match e.inner() {
            LinuxI2CError::Errno(n) => Some(*n),
            LinuxI2CError::Io(io) => io.raw_os_error(),
        }
    }

    impl i2c::Error for BusError {
        fn kind(&self) -> ErrorKind {
            if errno(&self.0) == Some(EREMOTEIO) {
                ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown)
            } else {
                self.0.kind()
            }
        }
    }

    impl ErrorType for Bus {
        type Error = BusError;
    }

    impl I2c for Bus {
        fn transaction(&mut self, address: u8, ops: &mut [Operation<'_>]) -> Result<(), BusError> {
            self.0.transaction(address, ops).map_err(BusError)
        }
    }

    type Step = Result<(), Error<BusError>>;

    fn path() -> String {
        std::env::var("MODULINO_I2C").unwrap_or_else(|_| "/dev/i2c-1".to_string())
    }

    fn open() -> I2cdev {
        I2cdev::new(path()).unwrap_or_else(|e| {
            eprintln!("cannot open {}: {e}", path());
            std::process::exit(2);
        })
    }

    fn expect(what: &str) {
        println!("expect: {what}");
    }

    /// Run `step` every 50 ms for 10 s.
    fn for_ten_seconds(mut step: impl FnMut() -> Step) -> Step {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) {
            step()?;
            sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    fn pause(ms: u64) {
        sleep(Duration::from_millis(ms));
    }

    /// Run `body`, then `cleanup` whatever `body` returned, so an error
    /// midway does not leave an actuator running. The body's error wins.
    fn with_cleanup<T>(
        dev: &mut T,
        body: impl FnOnce(&mut T) -> Step,
        cleanup: impl FnOnce(&mut T) -> Step,
    ) -> Step {
        let result = body(dev);
        let stopped = cleanup(dev);
        result.and(stopped)
    }

    pub fn main() -> ExitCode {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some(command) = args.first() else {
            eprintln!("usage: bench <command> [address]\ncommands: {COMMANDS}");
            return ExitCode::from(2);
        };
        let addr = match args.get(1) {
            None => None,
            Some(a) => {
                // Always hex, with or without 0x: `bench buttons 3e`.
                let digits = a
                    .strip_prefix("0x")
                    .or_else(|| a.strip_prefix("0X"))
                    .unwrap_or(a);
                let raw = u8::from_str_radix(digits, 16).ok();
                match raw.and_then(Address::new) {
                    Some(addr) => Some(addr),
                    None => {
                        eprintln!("not a 7-bit address: {a}");
                        return ExitCode::from(2);
                    }
                }
            }
        };
        if !COMMANDS.split_whitespace().any(|c| c == command) {
            eprintln!("unknown command {command}\ncommands: {COMMANDS}");
            return ExitCode::from(2);
        }
        if addr.is_some() && ["frames", "scan", "thermo"].contains(&command.as_str()) {
            eprintln!("{command} takes no address");
            return ExitCode::from(2);
        }
        if command == "nack" {
            return nack(addr);
        }
        if command == "frames" {
            frames();
            return ExitCode::SUCCESS;
        }
        let bus = Bus(open());
        let result = match command.as_str() {
            "scan" => scan_bus(bus),
            "hub" => hub(bus, addr),
            "buttons" => buttons(bus, addr),
            "buzzer" => buzzer(bus, addr),
            "vibro" => vibro(bus, addr),
            "knob" => knob(bus, addr),
            "pixels" => pixels(bus, addr),
            "joystick" => joystick(bus, addr),
            "opto-relay" => opto_relay(bus, addr),
            "latch-relay" => latch_relay(bus, addr),
            "led-matrix" => led_matrix(bus, addr),
            "motors" => motors(bus, addr, false),
            "motors-stepper" => motors(bus, addr, true),
            "thermo" => thermo(bus),
            "distance" => distance(bus, addr),
            "movement" => movement(bus, addr),
            "pressure" => pressure(bus, addr),
            "light" => light(bus, addr),
            other => unreachable!("{other} is in COMMANDS but has no arm"),
        };
        match result {
            Ok(()) => {
                println!("done");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:?}");
                ExitCode::FAILURE
            }
        }
    }

    /// What the host reports for a read nobody acknowledges, before and after
    /// the `Bus` mapping. Design section 7: "Does the Linux host report a
    /// NACK as `NoAcknowledge`?"
    fn nack(addr: Option<Address>) -> ExitCode {
        let raw = addr.map_or(0x08, Address::get);
        expect(&format!("nothing answers at {raw:#04x}; the read fails"));
        let mut buf = [0u8; 1];
        match open().read(raw, &mut buf) {
            Ok(()) => {
                println!("read at {raw:#04x} was acknowledged: pick an empty address");
                ExitCode::FAILURE
            }
            Err(e) => {
                println!("error:  {e}");
                println!("errno:  {:?}", errno(&e));
                println!("host kind: {:?}", i2c::Error::kind(&e));
                println!("bench kind: {:?}", i2c::Error::kind(&BusError(e)));
                ExitCode::SUCCESS
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{b:#04x}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The `i2ctransfer` commands for the open-question probes in
    /// docs/bench.md, with frames from the library's own encoders. These
    /// send what the drivers refuse to (short, over-length and `CF`
    /// frames), so they are printed for a person to run, not sent.
    fn frames() {
        use modulino_driver::codec::{buttons, buzzer, led_matrix, vibro};
        use modulino_driver::Kind;

        let path = path();
        let n = path.trim_start_matches("/dev/i2c-");
        let at = |k: Kind| k.default_address().get();

        println!("# Short write: Vibro on for 60 s, then the first 8 of the 12 off bytes");
        let on = vibro::encode_on(60_000, 60).unwrap();
        let off = vibro::encode_off();
        let v = at(Kind::Vibro);
        println!("i2ctransfer -y {n} w{}@{v:#04x} {}", on.len(), hex(&on));
        println!("i2ctransfer -y {n} w8@{v:#04x} {}", hex(&off[..8]));
        println!("# then the full off frame");
        println!("i2ctransfer -y {n} w{}@{v:#04x} {}", off.len(), hex(&off));

        println!("# Over-length write: Buttons LED A as 3 + 1 bytes, then LED B as 3 + 2, each with a read");
        let b = at(Kind::Buttons);
        for (extra, leds) in [
            (1usize, buttons::encode_leds(true, false, false)),
            (2, buttons::encode_leds(false, true, false)),
        ] {
            let mut frame = leds.to_vec();
            frame.extend(std::iter::repeat(0).take(extra));
            println!(
                "i2ctransfer -y {n} w{}@{b:#04x} {}",
                frame.len(),
                hex(&frame)
            );
            println!("i2ctransfer -y {n} r4@{b:#04x}");
        }

        println!("# Write-then-read race (board in mono mode): a border, then a read in the same transfer");
        let mut border = [[false; led_matrix::COLS]; led_matrix::ROWS];
        for (r, row) in border.iter_mut().enumerate() {
            for (c, px) in row.iter_mut().enumerate() {
                *px = r == 0 || r == led_matrix::ROWS - 1 || c == 0 || c == led_matrix::COLS - 1;
            }
        }
        let frame = led_matrix::encode_mono(&border);
        let m = at(Kind::LedMatrix);
        println!(
            "i2ctransfer -y {n} w{}@{m:#04x} {} r{}@{m:#04x}",
            frame.len(),
            hex(&frame),
            1 + led_matrix::READ_PAYLOAD
        );

        println!("# Prefix hazard (ASK FIRST: the board erases any stored address and reboots):");
        println!("# Buzzer at 17987 Hz, whose frame starts 'C' 'F'");
        let tone = buzzer::encode_tone(17987, 1000).unwrap();
        let z = at(Kind::Buzzer);
        println!("i2ctransfer -y {n} w{}@{z:#04x} {}", tone.len(), hex(&tone));
    }

    fn scan_bus(mut bus: Bus) -> Step {
        expect("one line per board; node boards show their strap byte");
        let mut found = [Found::Unknown(Address::new(1).unwrap()); 32];
        let n = scan(&mut bus, &mut found)?;
        for f in &found[..n.min(found.len())] {
            match *f {
                Found::Node(addr, kind) => {
                    let mut node = Node::new(&mut bus, addr, kind);
                    let strap = node.identify()?;
                    println!("{f:?} strap {:#04x}", strap.get());
                }
                _ => println!("{f:?}"),
            }
        }
        if n > found.len() {
            println!("{} more not shown", n - found.len());
        }
        Ok(())
    }

    fn hub(bus: Bus, addr: Option<Address>) -> Step {
        let bus = RefCell::new(bus);
        let hub = match addr {
            Some(a) => match Hub::with_address(&bus, a) {
                Some(hub) => hub,
                None => {
                    eprintln!("a hub address is 0x70-0x77");
                    std::process::exit(2);
                }
            },
            None => Hub::new(&bus),
        };
        expect("with only the hub on the Pi's bus, each port lists its own boards and no others");
        for port in 0..8 {
            let mut port_bus = hub.port(port).unwrap();
            let mut found = [Found::Unknown(Address::new(1).unwrap()); 16];
            let n = scan_except(&mut port_bus, &[hub.address()], &mut found)?;
            println!("port {port}: {:?}", &found[..n.min(found.len())]);
        }
        hub.deselect().map_err(Error::I2c)?;
        Ok(())
    }

    fn buttons(bus: Bus, addr: Option<Address>) -> Step {
        let mut b = match addr {
            Some(a) => Buttons::with_address(bus, a),
            None => Buttons::new(bus),
        };
        b.verify()?;
        expect("each LED lights while its button is held; presses print");
        with_cleanup(
            &mut b,
            |b| {
                let mut last = None;
                for_ten_seconds(|| {
                    let s = b.read()?;
                    b.set_leds(s.a, s.b, s.c)?;
                    if last != Some((s.a, s.b, s.c)) {
                        println!("a={} b={} c={}", s.a, s.b, s.c);
                        last = Some((s.a, s.b, s.c));
                    }
                    Ok(())
                })
            },
            |b| b.set_leds(false, false, false),
        )
    }

    fn buzzer(bus: Bus, addr: Option<Address>) -> Step {
        let mut b = match addr {
            Some(a) => Buzzer::with_address(bus, a),
            None => Buzzer::new(bus),
        };
        b.verify()?;
        expect("440 Hz for 0.5 s, silence, C5 for 0.5 s, then silence");
        with_cleanup(
            &mut b,
            |b| {
                b.tone(440, 500)?;
                pause(1000);
                b.play_note(Note::C5, 500)?;
                pause(1000);
                Ok(())
            },
            |b| b.no_tone(),
        )
    }

    fn vibro(bus: Bus, addr: Option<Address>) -> Step {
        let mut v = match addr {
            Some(a) => Vibro::with_address(bus, a),
            None => Vibro::new(bus),
        };
        v.verify()?;
        expect("medium buzz for 0.5 s; then a 5 s buzz that off() stops after 1 s");
        with_cleanup(
            &mut v,
            |v| {
                v.on(500, PowerLevel::Medium)?;
                pause(1500);
                v.on_with_power(5000, 60)?;
                pause(1000);
                v.off()?;
                expect("still now (off() sent 12 bytes)");
                pause(2000);
                Ok(())
            },
            |v| v.off(),
        )
    }

    fn knob(bus: Bus, addr: Option<Address>) -> Step {
        let mut k = match addr {
            Some(a) => Knob::with_address(bus, a),
            None => Knob::new(bus),
        };
        k.verify()?;
        k.set_position(0)?;
        expect("turning prints the position (starts at 0); pressing prints pressed");
        let mut last = None;
        for_ten_seconds(|| {
            let s = k.read()?;
            if last != Some((s.position, s.pressed)) {
                println!("position={} pressed={}", s.position, s.pressed);
                last = Some((s.position, s.pressed));
            }
            Ok(())
        })
    }

    fn pixels(bus: Bus, addr: Option<Address>) -> Step {
        let mut p = match addr {
            Some(a) => Pixels::with_address(bus, a),
            None => Pixels::new(bus),
        };
        p.verify()?;
        with_cleanup(
            &mut p,
            |p| {
                expect("all LEDs blue, dim, for 1 s");
                p.set_all(Color::BLUE, 10)?;
                p.show()?;
                pause(1000);
                expect("one red LED walks from the first to the last");
                for i in 0..8 {
                    p.clear_all();
                    p.set(i, Color::RED, 25)?;
                    p.show()?;
                    pause(300);
                }
                Ok(())
            },
            |p| {
                p.clear_all();
                p.show()
            },
        )?;
        expect("all off");
        Ok(())
    }

    fn joystick(bus: Bus, addr: Option<Address>) -> Step {
        let mut j = match addr {
            Some(a) => Joystick::with_address(bus, a),
            None => Joystick::new(bus),
        };
        j.verify()?;
        expect("centre prints (0, 0); record which way gives +x and +y; press prints pressed");
        let mut last = None;
        for_ten_seconds(|| {
            let s = j.read()?;
            if last != Some((s.x, s.y, s.pressed)) {
                println!("x={} y={} pressed={}", s.x, s.y, s.pressed);
                last = Some((s.x, s.y, s.pressed));
            }
            Ok(())
        })
    }

    fn opto_relay(bus: Bus, addr: Option<Address>) -> Step {
        let mut r = match addr {
            Some(a) => OptoRelay::with_address(bus, a),
            None => OptoRelay::new(bus),
        };
        r.verify()?;
        expect("relay on for 1 s (its LED lights), then off");
        with_cleanup(
            &mut r,
            |r| {
                r.on()?;
                println!("is_on after on(): {}", r.is_on()?);
                pause(1000);
                Ok(())
            },
            |r| r.off(),
        )?;
        println!("is_on after off(): {}", r.is_on()?);
        Ok(())
    }

    fn latch_relay(bus: Bus, addr: Option<Address>) -> Step {
        let mut r = match addr {
            Some(a) => LatchRelay::with_address(bus, a),
            None => LatchRelay::new(bus),
        };
        let mut delay = Delay;
        r.verify()?;
        println!("state at start: {:?}", r.state()?);
        expect("one click to set, 1 s, one click to reset");
        r.on(&mut delay)?;
        println!("state after on(): {:?}", r.state()?);
        pause(1000);
        r.off(&mut delay)?;
        println!("state after off(): {:?}", r.state()?);
        Ok(())
    }

    fn led_matrix(bus: Bus, addr: Option<Address>) -> Step {
        let mut m = match addr {
            Some(a) => LedMatrix::with_address(bus, a),
            None => LedMatrix::new(bus),
        };
        m.verify()?;
        println!("mode at start: {:?}", m.mode()?);
        with_cleanup(
            &mut m,
            |m| {
                expect("mono: a border around the 12 x 8 matrix for 2 s");
                let mut border = [[false; 12]; 8];
                for (r, row) in border.iter_mut().enumerate() {
                    for (c, px) in row.iter_mut().enumerate() {
                        *px = r == 0 || r == 7 || c == 0 || c == 11;
                    }
                }
                m.set_mode(MatrixMode::Mono)?;
                m.show_mono(&border)?;
                pause(2000);
                expect("grayscale: brightness rising across the columns, off to full, for 2 s; record which side is full");
                let mut ramp = [[0u8; 12]; 8];
                for row in ramp.iter_mut() {
                    for (c, level) in row.iter_mut().enumerate() {
                        *level = (c as u8 * 15) / 11;
                    }
                }
                m.set_mode(MatrixMode::Grayscale)?;
                m.show_gray(&ramp)?;
                pause(2000);
                Ok(())
            },
            |m| {
                m.set_mode(MatrixMode::Mono)?;
                m.show_mono(&[[false; 12]; 8])
            },
        )?;
        expect("blank, and left in mono mode");
        Ok(())
    }

    fn motors(bus: Bus, addr: Option<Address>, stepper: bool) -> Step {
        let mut m = match addr {
            Some(a) => Motors::with_address(bus, a),
            None => Motors::new(bus),
        };
        m.verify()?;
        if stepper {
            m.set_mode(MotorMode::Stepper)?;
            expect("the stepper turns 200 steps (one turn on a 200-step motor) in 1 s, then holds");
            with_cleanup(
                &mut m,
                |m| {
                    m.step(200, 50, 0)?;
                    for _ in 0..15 {
                        let s = m.status()?;
                        println!(
                            "busy={} a={} b={}",
                            s.busy, s.current_a_raw, s.current_b_raw
                        );
                        pause(100);
                    }
                    Ok(())
                },
                |m| m.release_coils(),
            )?;
            expect("coils released: the shaft turns freely by hand");
        } else {
            m.set_mode(MotorMode::Dc)?;
            expect(
                "A forward and B reverse at a quarter speed for 1 s, then the reverse, then stop",
            );
            with_cleanup(
                &mut m,
                |m| {
                    m.set_speeds(8192, -8192)?;
                    pause(500);
                    println!("{:?}", m.status()?);
                    pause(500);
                    m.set_speeds(-8192, 8192)?;
                    pause(1000);
                    Ok(())
                },
                |m| m.stop(),
            )?;
            println!("{:?}", m.status()?);
        }
        Ok(())
    }

    fn thermo(bus: Bus) -> Step {
        let mut t = Thermo::new(bus);
        let mut delay = Delay;
        let chip = t.verify()?;
        println!("chip: {chip:?} at {:?}", t.address());
        expect("room temperature and humidity, five readings 1 s apart");
        for _ in 0..5 {
            let r = t.read(&mut delay)?;
            println!("{:.2} C  {:.1} %RH", r.temperature, r.humidity);
            pause(1000);
        }
        Ok(())
    }

    fn distance(bus: Bus, addr: Option<Address>) -> Step {
        let mut d = match addr {
            Some(a) => Distance::with_address(bus, a),
            None => Distance::new(bus),
        };
        let mut delay = Delay;
        println!("variant: {:?}", d.verify()?);
        d.init(&mut delay)?;
        d.start_ranging(&mut delay)?;
        expect("hold a flat target at a measured distance: status 0, distance within the datasheet's accuracy");
        let mut readings = 0;
        let start = Instant::now();
        while readings < 20 && start.elapsed() < Duration::from_secs(10) {
            if d.data_ready()? {
                let r = d.read()?;
                println!(
                    "{} mm  status {}  signal {} kcps",
                    r.distance_mm, r.status, r.signal_kcps
                );
                readings += 1;
            }
            pause(20);
        }
        d.stop_ranging()?;
        if readings == 0 {
            println!("no reading in 10 s");
        }
        Ok(())
    }

    fn movement(bus: Bus, addr: Option<Address>) -> Step {
        let mut m = match addr {
            Some(a) => Movement::with_address(bus, a),
            None => Movement::new(bus),
        };
        m.init()?;
        println!("chip at {:?}", m.address());
        expect("flat and still: acceleration near (0, 0, 1) g, rotation near 0 dps");
        for _ in 0..10 {
            // A fresh sample each time, at 104 Hz.
            let start = Instant::now();
            while !(m.acceleration_ready()? && m.angular_velocity_ready()?) {
                if start.elapsed() > Duration::from_millis(100) {
                    println!("no new sample in 100 ms");
                    break;
                }
                pause(2);
            }
            let a = m.acceleration()?;
            let g = m.angular_velocity()?;
            println!(
                "accel {:+.2} {:+.2} {:+.2} g   gyro {:+.1} {:+.1} {:+.1} dps",
                a.x, a.y, a.z, g.x, g.y, g.z
            );
            pause(200);
        }
        Ok(())
    }

    fn pressure(bus: Bus, addr: Option<Address>) -> Step {
        let mut p = match addr {
            Some(a) => Pressure::with_address(bus, a),
            None => Pressure::new(bus),
        };
        p.init()?;
        expect("local air pressure (about 950-1050 hPa near sea level) and room temperature");
        for _ in 0..5 {
            pause(200);
            println!("{:.1} hPa  {:.1} C", p.pressure()?, p.temperature()?);
        }
        Ok(())
    }

    fn light(bus: Bus, addr: Option<Address>) -> Step {
        let mut l = match addr {
            Some(a) => Light::with_address(bus, a),
            None => Light::new(bus),
        };
        l.init()?;
        expect("lux rises under a lamp and falls when covered");
        for_ten_seconds(|| {
            if l.ready()? {
                let r = l.read()?;
                println!(
                    "{:.1} lux  r={} g={} b={} ir={}",
                    r.lux, r.red, r.green, r.blue, r.ir
                );
                pause(450);
            }
            Ok(())
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn kinds(errno: i32) -> (ErrorKind, ErrorKind) {
            let host = || I2CError::from(LinuxI2CError::Errno(errno));
            (
                i2c::Error::kind(&host()),
                i2c::Error::kind(&BusError(host())),
            )
        }

        #[test]
        fn eremoteio_becomes_a_nack() {
            // 121 is EREMOTEIO on arm and arm64, written out so the test does
            // not check the constant against itself. linux-embedded-hal
            // reports it as Other; the bench bus as a NACK.
            assert_eq!(
                kinds(121),
                (
                    ErrorKind::Other,
                    ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown)
                )
            );
        }

        #[test]
        fn other_errors_pass_through() {
            // ENXIO (6) is already an address NACK; EIO (5) is a bus error.
            let enxio = ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address);
            assert_eq!(kinds(6), (enxio, enxio));
            assert_eq!(kinds(5), (ErrorKind::Bus, ErrorKind::Bus));
        }

        #[test]
        fn an_io_eremoteio_becomes_a_nack_too() {
            let host = I2CError::from(LinuxI2CError::Io(std::io::Error::from_raw_os_error(121)));
            assert_eq!(
                i2c::Error::kind(&BusError(host)),
                ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown)
            );
        }

        #[test]
        fn cleanup_runs_after_a_failed_body_and_its_error_wins() {
            let mut stopped = false;
            let result = with_cleanup(
                &mut stopped,
                |_| Err(Error::Timeout),
                |stopped| {
                    *stopped = true;
                    Err(Error::DataError)
                },
            );
            assert!(stopped, "cleanup ran");
            assert!(matches!(result, Err(Error::Timeout)));
        }
    }
}
