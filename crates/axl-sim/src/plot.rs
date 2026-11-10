//! The plotter's data: named series of (time, value) points, a time window, and CSV export.
//!
//! Plain data, no window: the egui panel in `axl-app` draws it, and the tests here check the CSV by
//! parsing it back.

use std::collections::{BTreeMap, VecDeque};

/// Seconds of history kept per series (the plotter shows at most 60).
pub const HISTORY_S: f64 = 300.0;

/// One plotted channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Channel name.
    pub name: String,
    /// True for the built-in channels, false for those a controller sends with `axl_plot`.
    pub builtin: bool,
    /// (simulated seconds, value), oldest first.
    pub points: VecDeque<(f64, f64)>,
}

/// All series of a run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlotStore {
    series: Vec<Series>,
}

impl PlotStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget every point (a restart). Series names stay, so toggles survive a rewind.
    pub fn clear_points(&mut self) {
        for s in &mut self.series {
            s.points.clear();
        }
    }

    /// Add a point to a channel, creating it on first use. Points older than [`HISTORY_S`] before
    /// this one are dropped.
    pub fn push(&mut self, name: &str, builtin: bool, time: f64, value: f64) {
        let i = match self.series.iter().position(|s| s.name == name) {
            Some(i) => i,
            None => {
                self.series.push(Series {
                    name: name.to_owned(),
                    builtin,
                    points: VecDeque::new(),
                });
                self.series.len() - 1
            }
        };
        let s = &mut self.series[i];
        s.points.push_back((time, value));
        while s.points.front().is_some_and(|p| p.0 < time - HISTORY_S) {
            s.points.pop_front();
        }
    }

    /// Every series, in the order they first appeared.
    pub fn series(&self) -> &[Series] {
        &self.series
    }

    /// A series by name.
    pub fn get(&self, name: &str) -> Option<&Series> {
        self.series.iter().find(|s| s.name == name)
    }

    /// The newest time of any point.
    pub fn latest_time(&self) -> f64 {
        self.series
            .iter()
            .filter_map(|s| s.points.back().map(|p| p.0))
            .fold(0.0, f64::max)
    }

    /// The points of `name` with `from <= time <= to`.
    pub fn window(&self, name: &str, from: f64, to: f64) -> Vec<(f64, f64)> {
        self.get(name)
            .map(|s| {
                s.points
                    .iter()
                    .copied()
                    .filter(|p| p.0 >= from && p.0 <= to)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The value of `name` at `time` (the newest point at or before it), for the cursor readout.
    pub fn value_at(&self, name: &str, time: f64) -> Option<f64> {
        let s = self.get(name)?;
        s.points
            .iter()
            .rev()
            .find(|p| p.0 <= time)
            .or_else(|| s.points.front())
            .map(|p| p.1)
    }

    /// The channels in `channels` between `from` and `to` as CSV: a header `time_s,<names>`, then
    /// one row per distinct time with an empty cell where a channel has no point at that time.
    /// Names that contain a comma, a quote or a line break are quoted (RFC 4180).
    pub fn to_csv(&self, channels: &[&str], from: f64, to: f64) -> String {
        let mut rows: BTreeMap<u64, Vec<Option<f64>>> = BTreeMap::new();
        for (col, name) in channels.iter().enumerate() {
            for (t, v) in self.window(name, from, to) {
                // Times are never negative, so the bit patterns sort like the numbers.
                rows.entry(t.max(0.0).to_bits())
                    .or_insert_with(|| vec![None; channels.len()])[col] = Some(v);
            }
        }
        let mut out = String::from("time_s");
        for name in channels {
            out.push(',');
            out.push_str(&quote(name));
        }
        out.push('\n');
        for (bits, cells) in rows {
            out.push_str(&f64::from_bits(bits).to_string());
            for c in cells {
                out.push(',');
                if let Some(v) = c {
                    out.push_str(&v.to_string());
                }
            }
            out.push('\n');
        }
        out
    }
}

fn quote(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// A parsed CSV: the header cells and the rows (empty cells are `None`).
pub type ParsedCsv = (Vec<String>, Vec<Vec<Option<f64>>>);

/// Read back what [`PlotStore::to_csv`] wrote: the header cells, then the numeric rows (empty cells
/// are `None`). Any other CSV that follows RFC 4180 and has numeric data cells parses too.
pub fn parse_csv(text: &str) -> Result<ParsedCsv, String> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut cell = String::new();
    let mut row: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            }
            (true, c) => cell.push(c),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut cell)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut row));
            }
            (false, c) => cell.push(c),
        }
    }
    if quoted {
        return Err("a quoted cell is not closed".into());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        records.push(row);
    }
    let mut it = records.into_iter();
    let header = it.next().ok_or("the file is empty")?;
    let mut rows = Vec::new();
    for (n, r) in it.enumerate() {
        if r.len() != header.len() {
            return Err(format!(
                "row {} has {} cells, the header has {}",
                n + 1,
                r.len(),
                header.len()
            ));
        }
        rows.push(
            r.iter()
                .map(|c| {
                    if c.is_empty() {
                        Ok(None)
                    } else {
                        c.parse::<f64>()
                            .map(Some)
                            .map_err(|_| format!("row {}: {c:?} is not a number", n + 1))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok((header, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> PlotStore {
        let mut s = PlotStore::new();
        for i in 0..10 {
            let t = i as f64 * 0.1;
            s.push("speed (rad/s)", true, t, (t * 3.0).sin());
            if i % 2 == 0 {
                s.push("error, mm", false, t, 100.0 - i as f64);
            }
        }
        s.push("say \"hi\"", false, 0.25, 7.0);
        s
    }

    #[test]
    fn csv_round_trips_exactly() {
        let s = store();
        let names = ["speed (rad/s)", "error, mm", "say \"hi\""];
        let csv = s.to_csv(&names, 0.0, 10.0);
        let (header, rows) = parse_csv(&csv).unwrap();
        assert_eq!(
            header,
            ["time_s", "speed (rad/s)", "error, mm", "say \"hi\""]
        );
        // 10 speed times plus the odd 0.25 time.
        assert_eq!(rows.len(), 11);
        // Every original point comes back bit for bit in the right cell.
        let times: Vec<f64> = {
            let mut t: Vec<f64> = (0..10).map(|i| i as f64 * 0.1).collect();
            t.push(0.25);
            t.sort_by(f64::total_cmp);
            t
        };
        for (row, t) in rows.iter().zip(&times) {
            assert_eq!(row[0], Some(*t));
            for (col, name) in names.iter().enumerate() {
                let want = s
                    .get(name)
                    .and_then(|se| se.points.iter().find(|p| p.0 == *t))
                    .map(|p| p.1);
                assert_eq!(row[col + 1], want, "{name} at {t}");
            }
        }
    }

    #[test]
    fn csv_keeps_only_the_window_and_the_chosen_channels() {
        let s = store();
        let csv = s.to_csv(&["error, mm"], 0.35, 0.65);
        let (header, rows) = parse_csv(&csv).unwrap();
        assert_eq!(header, ["time_s", "error, mm"]);
        // Points at 0.4 and 0.6 only (every other tenth).
        let times: Vec<f64> = rows.iter().map(|r| r[0].unwrap()).collect();
        assert_eq!(times.len(), 2);
        assert!((times[0] - 0.4).abs() < 1e-12 && (times[1] - 0.6).abs() < 1e-12);
    }

    #[test]
    fn an_empty_selection_is_just_a_header() {
        let csv = store().to_csv(&[], 0.0, 1.0);
        assert_eq!(csv, "time_s\n");
        let (h, r) = parse_csv(&csv).unwrap();
        assert_eq!(h, ["time_s"]);
        assert!(r.is_empty());
    }

    #[test]
    fn parser_refuses_ragged_and_non_numeric_files() {
        assert!(parse_csv("a,b\n1\n").is_err());
        assert!(parse_csv("a,b\n1,x\n").is_err());
        assert!(parse_csv("a,\"b\n1,2\n").is_err());
        assert!(parse_csv("").is_err());
    }

    #[test]
    fn history_is_trimmed_and_restart_keeps_the_names() {
        let mut s = PlotStore::new();
        s.push("a", false, 0.0, 1.0);
        s.push("a", false, HISTORY_S + 1.0, 2.0);
        assert_eq!(s.get("a").unwrap().points.len(), 1);
        assert_eq!(s.latest_time(), HISTORY_S + 1.0);
        s.clear_points();
        assert!(s.get("a").unwrap().points.is_empty());
        assert_eq!(s.series().len(), 1);
    }

    #[test]
    fn cursor_value_is_the_newest_point_at_or_before() {
        let s = store();
        assert_eq!(s.value_at("error, mm", 0.35), Some(100.0 - 2.0));
        assert_eq!(s.value_at("nope", 0.0), None);
        // Before the first point: the first value.
        assert_eq!(s.value_at("speed (rad/s)", -1.0), Some(0.0));
    }
}
