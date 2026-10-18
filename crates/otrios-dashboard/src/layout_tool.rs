// SPDX-License-Identifier: Apache-2.0
//! Binary layout authoring utility. Publishing uses a loopback-only development API.
mod layout_seed;
use otrios_core::dashboard::layout::{DEFAULT_LAYOUT, Layout};
use otrios_interfaces::layout::*;
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
fn find(i: &mut Instance, id: u32) -> Option<&mut Instance> {
    if i.id == id {
        return Some(i);
    }
    i.children.iter_mut().find_map(|c| find(c, id))
}
fn http(
    method: &str,
    url: &str,
    body: &[u8],
    expected: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rest = url
        .strip_prefix("http://")
        .ok_or("CLI publishing requires loopback HTTP; remote clients use private HTTPS /layout")?;
    let (address, path) = rest.split_once('/').ok_or("URL needs /layout")?;
    let address: SocketAddr = address.parse()?;
    if !address.ip().is_loopback() || path != "layout" {
        return Err("CLI requires a loopback address and /layout".into());
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let precondition = expected
        .map(|id| format!("If-Match: \"{id}\"\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} /layout HTTP/1.0\r\nHost: {address}\r\nContent-Type: application/octet-stream\r\n{precondition}Content-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    let mut response = Vec::new();
    stream.take(256 * 1024).read_to_end(&mut response)?;
    let split = response
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or("Invalid HTTP reply")?;
    let status = std::str::from_utf8(&response[..split])?
        .split_whitespace()
        .nth(1)
        .ok_or("Missing HTTP status")?;
    if status != "200" {
        return Err(format!(
            "Layout endpoint {status}: {}",
            String::from_utf8_lossy(&response[split + 4..])
        )
        .into());
    }
    Ok(response[split + 4..].to_vec())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let at = |n: usize| args.get(n).map(String::as_str).ok_or("Missing argument");
    let command = at(0)?;
    if command == "seed" {
        fs::write(at(1)?, Layout::encode(&layout_seed::blueprint())?)?;
        return Ok(());
    }
    if command == "export" {
        fs::write(at(1)?, DEFAULT_LAYOUT)?;
        fs::write(
            format!("{}.base", at(1)?),
            Layout::from_bytes(DEFAULT_LAYOUT)?.hex_id(),
        )?;
        return Ok(());
    }
    if command == "fetch" {
        let bytes = http("GET", at(1)?, &[], None)?;
        Layout::from_bytes(&bytes)?;
        fs::write(
            format!("{}.base", at(2)?),
            Layout::from_bytes(&bytes)?.hex_id(),
        )?;
        fs::write(at(2)?, bytes)?;
        return Ok(());
    }
    let file = at(1)?;
    let layout = Layout::from_bytes(&fs::read(file)?)?;
    if command == "inspect" {
        println!(
            "{} · {} bytes",
            layout.blueprint().schema,
            layout.bytes().len()
        );
        for t in &layout.blueprint().templates {
            println!("Template {} {}", t.id, t.name);
            for p in &t.parameters {
                println!("  {} {} {:?}", p.key, p.name, p.value_type);
            }
        }
        fn print(i: &Instance, depth: usize) {
            println!(
                "{}Instance {} template={} frame={:?} overrides={:?}",
                "  ".repeat(depth),
                i.id,
                i.template,
                i.frame,
                i.properties
            );
            for c in &i.children {
                print(c, depth + 1);
            }
        }
        print(&layout.blueprint().root, 0);
        return Ok(());
    }
    if command == "publish" {
        let expected = fs::read_to_string(format!("{file}.base"))
            .map_err(|_| "Fetch/export a base layout before publishing")?;
        if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid base layout revision".into());
        }
        let response = http("POST", at(2)?, layout.bytes(), Some(&expected))?;
        fs::write(format!("{file}.base"), &response)?;
        println!("{}", String::from_utf8_lossy(&response));
        return Ok(());
    }
    let mut b = layout.blueprint().clone();
    let id: u32 = at(2)?.parse()?;
    if command == "remove" {
        fn remove(i: &mut Instance, id: u32) -> bool {
            if let Some(index) = i.children.iter().position(|v| v.id == id) {
                i.children.remove(index);
                return true;
            }
            i.children.iter_mut().any(|c| remove(c, id))
        }
        if !remove(&mut b.root, id) {
            return Err("Instance not found; root cannot be removed".into());
        }
    } else if command == "add-text" || command == "add-panel" {
        let parent: u32 = at(3)?.parse()?;
        let mut frame = [0.; 4];
        for (k, v) in frame.iter_mut().enumerate() {
            *v = at(4 + k)?.parse()?;
        }
        let item = Instance {
            id,
            template: if command == "add-text" { 2 } else { 3 },
            frame,
            properties: if command == "add-text" {
                vec![Property {
                    key: 1,
                    value: Expr::Literal(Value::Text(at(8)?.into())),
                }]
            } else {
                vec![]
            },
            children: vec![],
        };
        find(&mut b.root, parent)
            .ok_or("Parent instance not found")?
            .children
            .push(item);
    } else {
        let i = find(&mut b.root, id).ok_or("Instance not found")?;
        if command == "frame" {
            for (k, v) in i.frame.iter_mut().enumerate() {
                *v = at(3 + k)?.parse()?;
            }
        } else {
            let key: u16 = at(3)?.parse()?;
            let value=match command{"text"=>Value::Text(at(4)?.into()),"number"=>Value::Number(at(4)?.parse()?),"color"=>{let hex=at(4)?.trim_start_matches('#');if hex.len()!=8 || !hex.is_ascii(){return Err("Colour must be RRGGBBAA".into());}let mut c=[0;4];for(j,v)in c.iter_mut().enumerate(){*v=u8::from_str_radix(&hex[j*2..j*2+2],16)?;}Value::Color(c)},_=>return Err("Commands: seed/export/fetch/inspect/publish/frame/text/number/color/add-text/add-panel/remove".into())};
            i.properties.retain(|p| p.key != key);
            i.properties.push(Property {
                key,
                value: Expr::Literal(value),
            });
        }
    }
    fs::write(file, Layout::encode(&b)?)?;
    println!("Updated {file}; publish to activate after full validation.");
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn binary_seed_matches_authoring_fixture() {
        assert_eq!(
            otrios_core::dashboard::layout::Layout::encode(&super::layout_seed::blueprint())
                .unwrap(),
            otrios_core::dashboard::layout::DEFAULT_LAYOUT
        );
    }
}
