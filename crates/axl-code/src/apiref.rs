//! The API reference: every documented declaration of `axl.h`, grouped by its section heading.

/// The header the reference is parsed from (the SDK's own copy, so they cannot disagree).
pub const AXL_H: &str = include_str!("../../../sdk/c/axl.h");

/// One documented item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Function or constant name, for example `axl_motor_power`.
    pub name: String,
    /// The declaration as written, without the import attribute.
    pub signature: String,
    /// The doc comment text.
    pub doc: String,
}

/// A section such as "Motor commands".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Heading.
    pub title: String,
    /// Items in header order.
    pub entries: Vec<Entry>,
}

/// Parse a header's `/** ... */` comments, each tied to the declaration that follows it, into
/// sections taken from `/* ---- Title ---- */` lines. Doc comments on enum members and on
/// `#define`s become entries too (signature is the line itself).
pub fn parse(header: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut doc: Option<String> = None;
    for line in header.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("/* ----") {
            let title = rest.trim_end_matches("*/").trim().trim_matches('-').trim();
            sections.push(Section {
                title: title.to_owned(),
                entries: Vec::new(),
            });
            continue;
        }
        if t.starts_with("/**") && t.ends_with("*/") {
            doc = Some(
                t.trim_start_matches("/**")
                    .trim_end_matches("*/")
                    .trim()
                    .to_owned(),
            );
            continue;
        }
        let Some(d) = doc.take() else { continue };
        let sig = clean_signature(t);
        let Some(name) = declared_name(&sig) else {
            continue;
        };
        if sections.is_empty() {
            sections.push(Section {
                title: "Constants".into(),
                entries: Vec::new(),
            });
        }
        if let Some(s) = sections.last_mut() {
            s.entries.push(Entry {
                name,
                signature: sig,
                doc: d,
            });
        }
    }
    sections.retain(|s| !s.entries.is_empty());
    sections
}

/// The reference for the bundled header.
pub fn reference() -> Vec<Section> {
    parse(AXL_H)
}

fn clean_signature(line: &str) -> String {
    let mut s = line.to_owned();
    if let Some(i) = s.find("AXL_IMPORT(")
        && let Some(j) = s[i..].find(") ")
    {
        s.replace_range(i..i + j + 2, "");
    }
    s.trim()
        .trim_end_matches('{')
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim()
        .to_owned()
}

fn declared_name(sig: &str) -> Option<String> {
    if let Some(rest) = sig.strip_prefix("#define ") {
        return rest.split(['(', ' ']).next().map(str::to_owned);
    }
    if let Some(i) = sig.find('(') {
        let head = &sig[..i];
        return head
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .filter(|n| !n.is_empty())
            .map(str::to_owned);
    }
    // An enum member: `AXL_RUNNING = 0`.
    let name = sig.split(['=', ' ']).next()?;
    (name.starts_with("AXL_")).then(|| name.to_owned())
}

/// Entries whose name or text contains `query` (case-insensitive).
pub fn search<'a>(sections: &'a [Section], query: &str) -> Vec<(&'a str, &'a Entry)> {
    let q = query.to_lowercase();
    sections
        .iter()
        .flat_map(|s| s.entries.iter().map(move |e| (s.title.as_str(), e)))
        .filter(|(_, e)| {
            q.is_empty() || e.name.to_lowercase().contains(&q) || e.doc.to_lowercase().contains(&q)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_header_gives_a_reference() {
        let r = reference();
        let all: Vec<&Entry> = r.iter().flat_map(|s| &s.entries).collect();
        for name in [
            "axl_init",
            "axl_step",
            "axl_motor_power",
            "axl_motor_position",
            "axl_distance",
            "axl_color_name",
            "axl_touch_pressed",
            "axl_imu_gyro",
            "axl_battery_voltage",
            "axl_plot",
            "axl_device_named",
            "AXL_RUNNING",
            "AXL_ABI_VERSION",
        ] {
            assert!(all.iter().any(|e| e.name == name), "missing {name}");
        }
        let power = all.iter().find(|e| e.name == "axl_motor_power").unwrap();
        assert_eq!(
            power.signature,
            "void axl_motor_power(int32_t dev, double p)"
        );
        assert!(power.doc.starts_with("Drive a motor with power p"));
        assert!(r.iter().any(|s| s.title == "Motor commands"));
        // Every function the host provides is documented.
        assert!(all.len() >= 29);
    }

    #[test]
    fn search_matches_names_and_text() {
        let r = reference();
        assert!(
            search(&r, "brake")
                .iter()
                .any(|(_, e)| e.name == "axl_motor_brake")
        );
        assert!(
            search(&r, "PWM")
                .iter()
                .any(|(_, e)| e.name == "axl_motor_power")
        );
        assert!(search(&r, "zzzz").is_empty());
    }
}
