# modulino-driver

A layered, hardware-agnostic `no_std` Rust driver for the Arduino Modulino
boards, for any host with an `embedded-hal` 1.0 I2C implementation.

**Status: pre-release.** The crate is being restructured around the node
protocol the Modulino firmware speaks; [docs/design.md](docs/design.md) has
the architecture and milestones. Until 0.1.0 the API below is the one
inherited from modulino-rs 0.3.0 and will change.

## Origin

This crate started from [modulino-rs](https://github.com/melastmohican/modulino-rs)
0.3.0 by Mariusz Jurgielewicz, and keeps its history and its MIT OR
Apache-2.0 licence.

## Supported modules

| Module | Hardware | Implementation | Default address |
| :--- | :--- | :--- | :--- |
| `Buttons` | STM32C011 node | **Internal** | 0x3E |
| `Buzzer` | STM32C011 node | **Internal** | 0x1E |
| `Pixels` | STM32C011 node, APA102-style LEDs | **Internal** | 0x36 |
| `Distance` | VL53L4CD (VL53L4ED variant not yet supported) | **Internal** | 0x29 |
| `Movement` | LSM6DSOX | **Internal** | 0x6A/0x6B |
| `Knob` | STM32C011 node | **Internal** | 0x3A/0x3B |
| `Thermo` | HS3003 (SHT31 variant at 0x45 not yet supported) | **External** (`hs3003`) | 0x44 |
| `Joystick` | STM32C011 node | **Internal** | 0x2C |
| `LatchRelay` | STM32C011 node | **Internal** | 0x02 |
| `Vibro` | STM32C011 node | **Internal** | 0x38 |
| `LedMatrix` | STM32C011 node | **Internal** | 0x39 |
| `Pressure` | LPS22HB | **Internal (Experimental)** | 0x5C |
| `Light` | LTR-381RGB | **Internal** | 0x53 |
| `OptoRelay` | STM32C011 node | **Internal (Experimental)** | 0x14 |
| `Hub` | TCA9548A | **Internal** | 0x70 |
| `Motors` | STM32C011 node | **Internal (Experimental)** | 0x24 |

A node board runs Arduino's `node_modulino_firmware` on an STM32C011 and
drives its LEDs, buttons or motors itself; the host talks only to the
STM32. Its default address is its strap byte shifted right by one: the
straps are at `Core/Src/main.c:32-42`, packed into bits 6..1 at `819-845`,
and used as the HAL own-address at `652`, all at `7d578e0`. Arduino's
library also handles the SHT31 and VL53L4ED variants (`Arduino_Modulino` `src/Modulino.h:614-627`,
`946-965` at `24d08a9`).

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
modulino-driver = { git = "https://github.com/CameronBrooks11/modulino-driver" }
```

### Example: RGB LEDs

```rust
use modulino_driver::{Pixels, Color};

let mut pixels = Pixels::new(i2c);
pixels.verify()?;

// Set all LEDs to blue at 25% brightness, then the first to red at 50%
pixels.set_all(Color::BLUE, 25)?;
pixels.set(0, Color::RED, 50)?;

// Apply the changes
pixels.show()?;
```

### Example: Buttons

```rust
use modulino_driver::Buttons;

// Constructors do no I/O; verify() checks the board's identity.
let mut buttons = Buttons::new(i2c);
buttons.verify()?;

loop {
    // Light each LED while its button is pressed
    let state = buttons.read()?;
    buttons.set_leds(state.a, state.b, state.c)?;
}
```

### Example: Buzzer

```rust
use modulino_driver::{Buzzer, Note};

let mut buzzer = Buzzer::new(i2c);

// Play a tone at 440 Hz for 500 ms (outside 180-5898150 Hz is OutOfRange)
buzzer.tone(440, 500)?;

// Play a musical note
buzzer.play_note(Note::C5, 1000)?;

// Stop the tone
buzzer.no_tone()?;
```

### Example: Temperature & Humidity

```rust
use modulino_driver::Thermo;

let mut thermo = Thermo::new(i2c);

// Read temperature and humidity (requires a delay provider)
let measurement = thermo.read(&mut delay)?;
println!("Temperature: {:.1}°C", measurement.temperature);
println!("Humidity: {:.1}%", measurement.humidity);
```

### Example: Distance Sensor

```rust
use modulino_driver::Distance;

let mut distance = Distance::new(i2c)?;

// Start continuous ranging
distance.start_ranging()?;

// Read distance in millimeters
let mm = distance.read_distance_blocking()?;
println!("Distance: {} mm", mm);
```

### Example: Rotary Encoder

```rust
use modulino_driver::Knob;

let mut knob = Knob::new(i2c);

// Call the current position 0; the offset is kept on the host
knob.set_position(0)?;

loop {
    let state = knob.read()?;
    println!("Position: {}, Pressed: {}", state.position, state.pressed);
}
```

### Example: Joystick

```rust
use modulino_driver::Joystick;

let mut joystick = Joystick::new(i2c);

// Dead zone around the centre, in raw counts (default 26)
joystick.set_deadzone(15);

loop {
    let state = joystick.read()?;
    if state.pressed {
        println!("Button pressed at ({}, {})", state.x, state.y);
    }
}
```

### Example: Vibration Motor

```rust
use modulino_driver::{Vibro, PowerLevel};

let mut vibro = Vibro::new(i2c);

// Vibrate at medium power for 500ms
vibro.on(500, PowerLevel::Medium)?;

// Or use a custom power level (0-100)
vibro.on_with_power(1000, 60)?;

// Stop
vibro.off()?;
```

### Example: Relay Control

```rust
use modulino_driver::LatchRelay;

// Factory address 0x02
let mut relay = LatchRelay::new(i2c);

// Set the relay. This waits 120 ms while the firmware pulses the coil:
// any transfer to the board in that window can drive the wrong coil.
relay.on(&mut delay)?;

// Last commanded state; None until the first command after power-up
if relay.state()? == Some(true) {
    println!("Relay is ON");
}

// Reset
relay.off(&mut delay)?;
```

### Example: IMU (Accelerometer/Gyroscope)

```rust
use modulino_driver::Movement;

let mut movement = Movement::new(i2c)?;

// Read acceleration (in g)
let accel = movement.acceleration()?;
println!("Accel: x={:.2}g, y={:.2}g, z={:.2}g", accel.x, accel.y, accel.z);

// Read angular velocity (in dps)
let gyro = movement.angular_velocity()?;
println!("Gyro: x={:.2}°/s, y={:.2}°/s, z={:.2}°/s", gyro.x, gyro.y, gyro.z);
```

### Example: LED Matrix

```rust
use modulino_driver::{LedMatrix, MatrixMode};

// 12 columns by 8 rows; verify() reads the board's mode
let mut matrix = LedMatrix::new(i2c);
matrix.verify()?;

// Monochrome: pixels[row][col]
let mut pixels = [[false; 12]; 8];
pixels[0][0] = true;
matrix.set_mode(MatrixMode::Mono)?;
matrix.show_mono(&pixels)?;

// Grayscale: levels[row][col], 0-15
let mut levels = [[0u8; 12]; 8];
levels[7][11] = 15;
matrix.set_mode(MatrixMode::Grayscale)?;
matrix.show_gray(&levels)?;
```

### Example: Pressure & Temperature

```rust
use modulino_driver::Pressure;

let mut pressure = Pressure::new(i2c);

// Read atmospheric pressure in hPa
let hpa = pressure.pressure()?;
println!("Pressure: {:.1} hPa", hpa);

// Read ambient temperature
let temp = pressure.temperature()?;
println!("Temperature: {:.1}°C", temp);
```

### Example: Color & Light Sensor

```rust
use modulino_driver::Light;

let mut light = Light::new(i2c);
light.init()?;

// Read all color channels and calculations
let measurement = light.read()?;
println!("Lux: {:.2}", measurement.lux);
println!("Color: {}", measurement.color_name()); // e.g. "VIVID RED"

// Access raw RGB and IR channels
println!("Red: {}, Green: {}, Blue: {}, IR: {}", 
    measurement.red, measurement.green, measurement.blue, measurement.ir);
```

### Example: Opto Relay Control

```rust
use modulino_driver::OptoRelay;

let mut relay = OptoRelay::new(i2c);

// Turn on the opto relay and read it back
relay.on()?;
assert!(relay.is_on()?);

// Turn off
relay.off()?;
```

### Example: Motors Control (Experimental)

```rust
use modulino_driver::{Motors, MotorMode};

let mut motors = Motors::new(i2c);
motors.verify()?;

// Two DC motors: speeds are -32767..32767
motors.set_mode(MotorMode::Dc)?;
motors.set_speeds(16384, -16384)?;
motors.stop()?;

// One stepper: 200 steps, one every 5 ms (50 x 0.1 ms), hold afterwards
motors.set_mode(MotorMode::Stepper)?;
motors.step(200, 50, 0)?;

// Currents and flags
let status = motors.status()?;
```

### Example: I2C Hub (Multiplexer)

```rust
use core::cell::RefCell;
use modulino_driver::{Address, Buttons, Hub};

// The hub borrows the bus per transaction, so wrap it in a RefCell
let bus = RefCell::new(i2c);
let hub = Hub::new(&bus); // 0x70; Hub::with_address for 0x71-0x77

// Each port is an I2C bus: it selects the port, runs the transaction and
// deselects again, even on error. Two identical boards can sit on two ports.
let mut left = Buttons::new(hub.port(0).unwrap());
let mut right = Buttons::new(hub.port(1).unwrap());
left.set_leds(true, false, false)?;
right.set_leds(false, false, true)?;

// A hub behind a hub: wrap the outer port in a RefCell
let outer = RefCell::new(hub.port(7).unwrap());
let inner = Hub::with_address(&outer, Address::new(0x71).unwrap()).unwrap();
```

## Discovery

```rust
use modulino_driver::{scan, Address, Found};

let mut found = [Found::Unknown(Address::new(1).unwrap()); 16];
let n = scan(&mut i2c, &mut found)?;
for f in &found[..n.min(found.len())] {
    // Node(addr, Kind), Chip(addr, ChipKind), Hub(addr) or Unknown(addr)
    println!("{:?}", f);
}
```

Each answer is identified by evidence: a hub by its control register, a
chip by its ID register, a node board by the strap byte it sends first,
never by its address alone. A Thermo board's HS3003 has no readable
identity and shows as `Unknown(0x44)`. Behind a hub, call `scan_except` on
the port and skip the hub's own address.

## Features

- `defmt`: Enable `defmt` formatting for error types (useful for embedded debugging)

```toml
[dependencies]
modulino-driver = { git = "https://github.com/CameronBrooks11/modulino-driver", features = ["defmt"] }
```

## Hardware Requirements

All Modulino devices communicate over I2C at 100kHz. They use the Qwiic/STEMMA QT connector standard for easy daisy-chaining.

## Custom I2C Addresses

Some modules can have their address changed. You can specify a custom address when creating a driver:

```rust
use modulino_driver::{Address, Buttons};

// Use default address
let buttons = Buttons::new(i2c);

// Use custom address (7-bit, 0x01..=0x77)
let buttons = Buttons::with_address(i2c, Address::new(0x40).unwrap());
```

## Minimum Supported Rust Version (MSRV)

The library requires Rust 1.75 or later. The test suite's dev-dependencies
need a newer toolchain.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

This library is inspired by the [Arduino Modulino MicroPython library](https://github.com/arduino/arduino-modulino-mpy)
and [Arduino Modulino C++ library](https://github.com/arduino-libraries/Arduino_Modulino)
