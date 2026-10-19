// The parts of a KiCad netlist (kicadsexpr format, version "E") that the
// emulator needs: each part's reference and fields, and each net's
// name and pins.
use std::error::Error;
use std::path::Path;
use std::process::Command;

use crate::kicad::sexpr::{ self, Sexpr };

pub struct Part {
    pub reference: String,
    pub fields: Vec<(String, String)>
}

impl Part {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.iter().find(|(field, _)| field == name).map(|(_, value)| value.as_str())
    }
}

pub struct Node {
    pub reference: String,
    // Pin number, e.g. "21".
    pub pin: String,
    // Pin name in the symbol, e.g. "INT/RB0"; KiCad leaves it out for
    // unnamed pins.
    pub function: Option<String>
}

pub struct Net {
    pub name: String,
    pub nodes: Vec<Node>
}

pub struct Netlist {
    pub parts: Vec<Part>,
    pub nets: Vec<Net>
}

fn required<'a>(expr: &'a Sexpr, name: &str, what: &str) -> Result<&'a str, String> {
    expr.value(name).ok_or_else(|| format!("{what} without {name}"))
}

pub fn parse(text: &str) -> Result<Netlist, Box<dyn Error>> {
    let export = sexpr::parse(text)?;
    if export.name() != Some("export") {
        return Err("not a KiCad netlist: expected (export ...)".into());
    }

    let mut parts = vec![];
    for comp in export.child("components").into_iter().flat_map(|c| c.children("comp")) {
        // (fields (field (name "Emu") "value") ...); an empty field has no value.
        let fields = comp.child("fields").into_iter().flat_map(|f| f.children("field"))
            .filter_map(|field| {
                let name = field.value("name")?;
                let value = field.args().last()?.atom().unwrap_or("");
                Some((name.to_string(), value.to_string()))
            }).collect();
        parts.push(Part {
            reference: required(comp, "ref", "component")?.to_string(),
            fields
        });
    }

    let mut nets = vec![];
    for net in export.child("nets").into_iter().flat_map(|n| n.children("net")) {
        let name = required(net, "name", "net")?;
        let mut nodes = vec![];
        for node in net.children("node") {
            nodes.push(Node {
                reference: required(node, "ref", &format!("node of net {name}"))?.to_string(),
                pin: required(node, "pin", &format!("node of net {name}"))?.to_string(),
                function: node.value("pinfunction").map(str::to_string)
            });
        }
        nets.push(Net { name: name.to_string(), nodes });
    }

    Ok(Netlist { parts, nets })
}

// Reads a netlist file, or exports one from a schematic (.kicad_sch) with
// kicad-cli.
pub fn read(path: &Path) -> Result<Netlist, Box<dyn Error>> {
    let text = if path.extension().is_some_and(|ext| ext == "kicad_sch") {
        let dir = tempfile::tempdir()?;
        let out = dir.path().join("board.net");
        let result = Command::new("kicad-cli").args(["sch", "export", "netlist", "-o"]).arg(&out).arg(path).output()
            .map_err(|err| format!("can't run kicad-cli to export the netlist: {err}"))?;
        if !result.status.success() {
            return Err(format!("kicad-cli failed on {}:\n{}{}", path.display(),
                               String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr)).into());
        }
        std::fs::read_to_string(&out)?
    } else {
        std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?
    };
    parse(&text).map_err(|err| format!("{}: {err}", path.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shortened from a kicad-cli 9 export.
    const NETLIST: &str = r#"(export (version "E")
  (design (source "x.kicad_sch") (tool "Eeschema 9.0.8"))
  (components
    (comp (ref "C1")
      (value "15pF")
      (fields
        (field (name "Footprint"))
        (field (name "Datasheet")))
      (libsource (lib "Device") (part "C") (description "Unpolarized capacitor")))
    (comp (ref "J2")
      (value "CONTROL")
      (fields
        (field (name "Emu") "74C922 pins=\"1:D0 2:D1\" hold=130ms")
        (field (name "Footprint")))
      (property (name "Emu") (value "74C922 pins=\"1:D0 2:D1\" hold=130ms"))))
  (nets
    (net (code "1") (name "+5V") (class "Default")
      (node (ref "C1") (pin "1") (pintype "passive"))
      (node (ref "J2") (pin "9") (pinfunction "Pin_9") (pintype "passive")))
    (net (code "2") (name "/KEY_D0") (class "Default")
      (node (ref "J2") (pin "1") (pinfunction "Pin_1") (pintype "passive")))))
"#;

    #[test]
    fn reads_parts_fields_and_nets() {
        let netlist = parse(NETLIST).unwrap();
        let refs: Vec<&str> = netlist.parts.iter().map(|p| p.reference.as_str()).collect();
        assert_eq!(refs, ["C1", "J2"]);
        assert_eq!(netlist.parts[0].field("Footprint"), Some(""));
        assert_eq!(netlist.parts[0].field("Emu"), None);
        assert_eq!(netlist.parts[1].field("Emu"), Some("74C922 pins=\"1:D0 2:D1\" hold=130ms"));

        let names: Vec<&str> = netlist.nets.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["+5V", "/KEY_D0"]);
        let nodes: Vec<(&str, &str, Option<&str>)> = netlist.nets[0].nodes.iter()
            .map(|n| (n.reference.as_str(), n.pin.as_str(), n.function.as_deref())).collect();
        assert_eq!(nodes, [("C1", "1", None), ("J2", "9", Some("Pin_9"))]);
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(parse("(kicad_sch (version 1))").err().unwrap().to_string(), "not a KiCad netlist: expected (export ...)");
    }
}
