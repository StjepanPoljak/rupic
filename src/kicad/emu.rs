// The Emu field of a schematic symbol: which model emulates the part, with
// its parameters, e.g. `PIC16F876 rom=TETRIS.hex fosc=4MHz` or
// `74C922 pins="1:D0 2:D1" keys="a:3 space:5"`.
use std::collections::HashMap;
use std::path::Path;

use crate::common::byte_data::ByteData;
use crate::common::component::Component;
use crate::common::mcu::MCU;
use crate::common::time::{ Time, PS, NS, US, MS, S };
use crate::hw::keypad::Keypad;
use crate::hw::st7920::ST7920;
use crate::pic::pic16f876::PIC16F876;

// ST7920 12864 LCD module header.
const ST7920_PINOUT: &[(&str, &str)] = &[
    ("4", "RS"), ("5", "RW"), ("6", "E"),
    ("7", "DB0"), ("8", "DB1"), ("9", "DB2"), ("10", "DB3"),
    ("11", "DB4"), ("12", "DB5"), ("13", "DB6"), ("14", "DB7")
];
// MM74C922 DIP-18: data outputs A..D and DA.
const MM74C922_PINOUT: &[(&str, &str)] = &[("17", "D0"), ("16", "D1"), ("15", "D2"), ("14", "D3"), ("12", "DA")];
// Hold time of a key press; a terminal only reports presses.
const DEFAULT_KEY_HOLD: Time = 130 * MS;

pub struct Emu {
    pub model: String,
    params: Vec<(String, String)>
}

pub struct Model {
    pub component: Box<dyn Component>,
    // Symbol pin number -> component pin name. Without it, pins are matched
    // by name.
    pub pinout: Option<HashMap<String, String>>
}

// Splits on whitespace; double quotes group words, as in keys="a:3 d:4".
fn words(text: &str) -> Result<Vec<String>, String> {
    let mut words = vec![];
    let mut word = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            },
            c => word.push(c)
        }
    }
    if quoted {
        return Err("unterminated quote".into());
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

// Splits a number from its unit: "130ms" -> (130, "ms").
fn number_unit(text: &str) -> Result<(u64, &str), String> {
    let split = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    let number = text[..split].parse().map_err(|_| format!("{text} doesn't start with a number"))?;
    Ok((number, &text[split..]))
}

fn parse_time(text: &str) -> Result<Time, String> {
    let (number, unit) = number_unit(text)?;
    let unit = match unit {
        "ps" => PS, "ns" => NS, "us" => US, "ms" => MS, "s" => S,
        _ => return Err(format!("{text}: expected a time like 130ms"))
    };
    Ok(number * unit)
}

fn parse_hz(text: &str) -> Result<u64, String> {
    let (number, unit) = number_unit(text)?;
    let unit = match unit {
        "" | "Hz" => 1, "kHz" => 1_000, "MHz" => 1_000_000,
        _ => return Err(format!("{text}: expected a frequency like 4MHz"))
    };
    Ok(number * unit)
}

// "a:3 space:5" -> key a gives code 3, the space bar code 5.
fn parse_keys(text: &str) -> Result<Vec<(u8, u8)>, String> {
    words(text)?.iter().map(|entry| {
        let (key, code) = entry.split_once(':').ok_or_else(|| format!("{entry}: expected KEY:CODE"))?;
        let key = match key {
            "space" => b' ',
            key if key.len() == 1 => key.as_bytes()[0],
            _ => return Err(format!("{entry}: a key is one character or space"))
        };
        let code = code.parse().ok().filter(|code| *code < 16).ok_or_else(|| format!("{entry}: code must be 0..15"))?;
        Ok((key, code))
    }).collect()
}

impl Emu {
    pub fn parse(field: &str) -> Result<Self, String> {
        let mut words = words(field)?.into_iter();
        let model = words.next().ok_or("empty Emu field")?;
        let params = words.map(|word| {
            word.split_once('=').map(|(key, value)| (key.to_string(), value.to_string()))
                .ok_or_else(|| format!("{word}: expected KEY=VALUE"))
        }).collect::<Result<_, _>>()?;
        Ok(Self { model, params })
    }

    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, value)| value.as_str())
    }

    fn check_params(&self, known: &[&str]) -> Result<(), String> {
        match self.params.iter().find(|(key, _)| key != "pins" && !known.contains(&key.as_str())) {
            Some((key, _)) => Err(format!("{} has no parameter {key}", self.model)),
            None => Ok(())
        }
    }

    // Builds the model. Paths are relative to dir, the schematic's directory.
    pub fn build(&self, dir: &Path) -> Result<Model, String> {
        let (component, pinout): (Box<dyn Component>, &[(&str, &str)]) = match self.model.as_str() {
            "PIC16F876" => {
                self.check_params(&["rom", "fosc"])?;
                let rom = dir.join(self.param("rom").ok_or("PIC16F876 needs rom=FILE.hex")?);
                let rom = ByteData::new_from_intel_hex(rom.to_str().ok_or("ROM path isn't UTF-8")?)
                    .map_err(|err| format!("{}: {err}", rom.display()))?;
                let mut pic = PIC16F876::new();
                pic.load_rom(&rom);
                if let Some(fosc) = self.param("fosc") {
                    pic.set_fosc(parse_hz(fosc)?);
                }
                (Box::new(pic), &[])
            },
            "ST7920" => {
                self.check_params(&[])?;
                (Box::new(ST7920::new()), ST7920_PINOUT)
            },
            "74C922" => {
                self.check_params(&["hold", "keys"])?;
                let mut keypad = Keypad::new(self.param("hold").map(parse_time).transpose()?.unwrap_or(DEFAULT_KEY_HOLD));
                for (key, code) in parse_keys(self.param("keys").unwrap_or(""))? {
                    keypad.key_to_pin(key, code);
                }
                (Box::new(keypad), MM74C922_PINOUT)
            },
            model => return Err(format!("unknown model {model}"))
        };

        let pinout = match self.param("pins") {
            Some(pins) => {
                let pins = words(pins)?.iter().map(|entry| {
                    let (number, name) = entry.split_once(':').ok_or_else(|| format!("{entry}: expected NUMBER:PIN"))?;
                    if !component.pins().contains(&name) {
                        return Err(format!("{} has no pin {name}", self.model));
                    }
                    Ok((number.to_string(), name.to_string()))
                }).collect::<Result<_, String>>()?;
                Some(pins)
            },
            None if pinout.is_empty() => None,
            None => Some(pinout.iter().map(|(number, name)| (number.to_string(), name.to_string())).collect())
        };
        Ok(Model { component, pinout })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_and_quoted_params() {
        let emu = Emu::parse(r#"74C922  pins="1:D0 2:D1" hold=5ms"#).unwrap();
        assert_eq!(emu.model, "74C922");
        assert_eq!(emu.param("pins"), Some("1:D0 2:D1"));
        assert_eq!(emu.param("hold"), Some("5ms"));
        assert_eq!(Emu::parse("").err().unwrap(), "empty Emu field");
        assert_eq!(Emu::parse("X a").err().unwrap(), "a: expected KEY=VALUE");
        assert_eq!(Emu::parse("X a=\"b").err().unwrap(), "unterminated quote");
    }

    #[test]
    fn parses_units_and_keys() {
        assert_eq!(parse_time("130ms"), Ok(130 * MS));
        assert_eq!(parse_hz("4MHz"), Ok(4_000_000));
        assert_eq!(parse_hz("32768"), Ok(32768));
        assert!(parse_time("130").is_err());
        assert_eq!(parse_keys("a:3 space:5"), Ok(vec![(b'a', 3), (b' ', 5)]));
        assert!(parse_keys("ab:3").is_err());
        assert!(parse_keys("a:16").is_err());
    }

    #[test]
    fn builds_models_with_pinouts() {
        let lcd = Emu::parse("ST7920").unwrap().build(Path::new(".")).unwrap();
        assert_eq!(lcd.pinout.unwrap()["4"], "RS");

        let keypad = Emu::parse(r#"74C922 pins="3:DA""#).unwrap().build(Path::new(".")).unwrap();
        assert_eq!(keypad.pinout.unwrap(), HashMap::from([("3".to_string(), "DA".to_string())]));

        let err = |field: &str| Emu::parse(field).unwrap().build(Path::new(".")).err().unwrap();
        assert_eq!(err("74C922 pins=\"3:Q\""), "74C922 has no pin Q");
        assert_eq!(err("ST7920 fosc=1MHz"), "ST7920 has no parameter fosc");
        assert_eq!(err("PIC16F876"), "PIC16F876 needs rom=FILE.hex");
        assert_eq!(err("Z80"), "unknown model Z80");
    }
}
