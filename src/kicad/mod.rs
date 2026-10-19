// Builds a board from a KiCad schematic or netlist. Parts with an Emu field
// are emulated, everything else (resistors, capacitors, crystals, ...) is
// left out. Nets keep their KiCad names; power nets (GND, +5V, ...) get a
// constant driver; nets called NAME0..NAMEn are also traced as bus NAME.
pub mod sexpr;
pub mod netlist;
pub mod emu;

use std::collections::{ BTreeMap, HashMap, HashSet };
use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::board::Board;
use crate::common::net::Level;
use crate::hw::rail::Rail;
use crate::kicad::emu::Emu;
use crate::kicad::netlist::Netlist;

// What load() made of a netlist.
#[derive(Default)]
pub struct Report {
    // (reference, model)
    pub emulated: Vec<(String, String)>,
    pub ignored: Vec<String>,
    pub rails: Vec<(String, Level)>,
    pub buses: Vec<String>,
    // Nets with a single emulated pin: nothing to talk to in the emulator.
    pub lone_nets: Vec<String>
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let emulated: Vec<String> = self.emulated.iter().map(|(reference, model)| format!("{reference} ({model})")).collect();
        let rails: Vec<String> = self.rails.iter().map(|(net, level)| format!("{net} ({})", if *level == Level::High { 1 } else { 0 })).collect();
        writeln!(f, "Emulated: {}", emulated.join(", "))?;
        writeln!(f, "Not emulated: {}", self.ignored.join(", "))?;
        writeln!(f, "Power rails: {}", rails.join(", "))?;
        writeln!(f, "Buses: {}", self.buses.join(", "))?;
        write!(f, "Nets with one emulated pin: {}", self.lone_nets.join(", "))
    }
}

// Level of a power net, by its name.
fn rail_level(net: &str) -> Option<Level> {
    if net.starts_with("GND") || net == "VSS" || net == "0V" {
        Some(Level::Low)
    } else if net.starts_with('+') || net == "VCC" || net == "VDD" {
        Some(Level::High)
    } else {
        None
    }
}

// Matches a symbol pin name such as "INT/RB0" or "~{MCLR}/Vpp" against the
// model's pins.
fn match_pin(function: &str, pins: &[&'static str]) -> Option<&'static str> {
    function.split('/').map(|name| name.replace("~{", "").replace('}', ""))
        .find_map(|name| pins.iter().find(|pin| **pin == name).copied())
}

// Splits "LCD_DB7" into ("LCD_DB", 7).
fn bus_member(net: &str) -> Option<(&str, usize)> {
    let prefix = net.trim_end_matches(|c: char| c.is_ascii_digit());
    if prefix.is_empty() || prefix.len() == net.len() {
        return None;
    }
    Some((prefix, net[prefix.len()..].parse().ok()?))
}

pub fn load(board: &mut Board, path: &Path) -> Result<Report, Box<dyn Error>> {
    let netlist = netlist::read(path)?;
    build(board, &netlist, path.parent().unwrap_or(Path::new(".")))
}

// Paths in Emu fields are relative to dir.
pub fn build(board: &mut Board, netlist: &Netlist, dir: &Path) -> Result<Report, Box<dyn Error>> {
    let mut report = Report::default();

    // reference -> (pinout, component pins)
    let mut parts = HashMap::new();
    for part in &netlist.parts {
        let Some(field) = part.field("Emu").filter(|field| !field.trim().is_empty()) else {
            report.ignored.push(part.reference.clone());
            continue;
        };
        let emu = Emu::parse(field).map_err(|err| format!("{}: {err}", part.reference))?;
        let model = emu.build(dir).map_err(|err| format!("{}: {err}", part.reference))?;
        parts.insert(part.reference.as_str(), (model.pinout, model.component.pins().to_vec()));
        board.add_component(&part.reference, model.component);
        report.emulated.push((part.reference.clone(), emu.model));
    }

    let mut connected = HashSet::new();
    let mut net_names = vec![];
    for net in &netlist.nets {
        // KiCad gives each pin with a no-connect flag a net of its own.
        if net.name.starts_with("unconnected-") {
            continue;
        }
        let name = net.name.strip_prefix('/').unwrap_or(&net.name);
        let mut pins = vec![];
        for node in &net.nodes {
            let Some((pinout, model_pins)) = parts.get(node.reference.as_str()) else {
                continue;
            };
            let pin = match pinout {
                Some(pinout) => pinout.get(&node.pin).map(String::as_str),
                None => node.function.as_deref().and_then(|function| match_pin(function, model_pins))
            };
            let Some(pin) = pin else {
                continue;
            };
            let pin = format!("{}.{pin}", node.reference);
            if !connected.insert(pin.clone()) {
                return Err(format!("{pin} is on more than one net (last: {name})").into());
            }
            pins.push(pin);
        }
        if pins.is_empty() {
            continue;
        }

        if let Some(level) = rail_level(name) {
            board.add_component(name, Box::new(Rail::new(level)));
            pins.push(format!("{name}.OUT"));
            report.rails.push((name.to_string(), level));
        } else if pins.len() == 1 {
            report.lone_nets.push(name.to_string());
        }
        let pins: Vec<&str> = pins.iter().map(String::as_str).collect();
        board.net(name, &pins);
        net_names.push(name);
    }

    // Nets NAME0..NAMEn, without gaps, become bus NAME.
    let mut buses: BTreeMap<&str, Vec<(usize, &str)>> = BTreeMap::new();
    for net in &net_names {
        if let Some((prefix, idx)) = bus_member(net) {
            buses.entry(prefix).or_default().push((idx, net));
        }
    }
    for (prefix, mut members) in buses {
        members.sort_unstable();
        if members.len() < 2 || members.iter().enumerate().any(|(i, (idx, _))| i != *idx) {
            continue;
        }
        let names: Vec<&str> = members.iter().map(|(_, net)| *net).collect();
        board.group_bus(prefix, &names);
        report.buses.push(prefix.to_string());
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board_test::SharedBuf;

    // U1 is matched by pin name, J2 through its pins= map.
    fn netlist(j2_pins: &str) -> String {
        format!(r#"(export (version "E")
  (components
    (comp (ref "R1") (fields (field (name "Footprint"))))
    (comp (ref "J2") (fields (field (name "Emu") "74C922 pins=\"{j2_pins}\"")))
    (comp (ref "U1") (fields (field (name "Emu") "PIC16F876 rom=TETRIS.hex"))))
  (nets
    (net (code "1") (name "+5V")
      (node (ref "J2") (pin "5") (pinfunction "Pin_5"))
      (node (ref "R1") (pin "1")))
    (net (code "2") (name "GND")
      (node (ref "U1") (pin "8") (pinfunction "VSS")))
    (net (code "3") (name "/D0")
      (node (ref "J2") (pin "1") (pinfunction "Pin_1"))
      (node (ref "U1") (pin "21") (pinfunction "INT/RB0")))
    (net (code "4") (name "/D1")
      (node (ref "J2") (pin "2") (pinfunction "Pin_2"))
      (node (ref "U1") (pin "22") (pinfunction "RB1")))
    (net (code "5") (name "/X0")
      (node (ref "U1") (pin "11") (pinfunction "RC0/T1OSO/T1CKI")))
    (net (code "6") (name "/X2")
      (node (ref "U1") (pin "13") (pinfunction "RC2/CCP1")))
    (net (code "7") (name "/MCLR")
      (node (ref "U1") (pin "1") (pinfunction "~{{MCLR}}/Vpp")))
    (net (code "8") (name "unconnected-(U1-RC1-Pad12)")
      (node (ref "U1") (pin "12") (pinfunction "RC1/T1OSI/CCP2")))))
"#)
    }

    fn load_text(board: &mut Board, text: &str) -> Result<Report, Box<dyn Error>> {
        build(board, &netlist::parse(text).unwrap(), Path::new("."))
    }

    #[test]
    fn loads_parts_nets_rails_and_buses() {
        let mut board = Board::new_headless();
        let report = load_text(&mut board, &netlist("1:D0 2:D1 5:DA")).unwrap();
        assert_eq!(report.to_string(), "\
Emulated: J2 (74C922), U1 (PIC16F876)
Not emulated: R1
Power rails: +5V (1)
Buses: D
Nets with one emulated pin: X0, X2");
        // VSS isn't an emulated pin, so GND has nothing to drive.
        assert_eq!(board.net_level("GND"), None);
        assert_eq!(board.net_level("MCLR"), None);
        assert_eq!(board.find_component("+5V").is_some(), true);

        let buf = SharedBuf::default();
        board.trace(Box::new(buf.clone()), &["D", "+5V"]).unwrap();
        assert!(board.trace(Box::new(SharedBuf::default()), &["X"]).is_err(), "X0, X2 have a gap");
        board.init();
        board.step();
        assert_eq!(board.net_level("+5V"), Some(Level::High));
        assert!(buf.text().contains("$var wire 2 ! D [1:0] $end"), "{}", buf.text());
    }

    #[test]
    fn reports_bad_parts() {
        let mut board = Board::new_headless();
        let err = load_text(&mut board, &netlist("1:D0 2:D0")).err().unwrap();
        assert_eq!(err.to_string(), "J2.D0 is on more than one net (last: D1)");

        let mut board = Board::new_headless();
        let err = load_text(&mut board, &netlist("1:Q")).err().unwrap();
        assert_eq!(err.to_string(), "J2: 74C922 has no pin Q");

        let mut board = Board::new_headless();
        let text = netlist("1:D0").replace("PIC16F876 rom=TETRIS.hex", "PIC16F876 rom=missing.hex");
        let err = load_text(&mut board, &text).err().unwrap().to_string();
    assert!(err.starts_with("U1: ./missing.hex: "), "{err}");
    }

    #[test]
    fn matches_pins_by_name() {
        assert_eq!(match_pin("INT/RB0", &["RB0", "RB1"]), Some("RB0"));
        assert_eq!(match_pin("~{SS}/AN4/RA5", &["RA5"]), Some("RA5"));
        assert_eq!(match_pin("VSS", &["RA5"]), None);
        assert_eq!(bus_member("LCD_DB7"), Some(("LCD_DB", 7)));
        assert_eq!(bus_member("LCD_E"), None);
        assert_eq!(bus_member("42"), None);
    }
}
