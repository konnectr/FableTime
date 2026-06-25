//! Pure (no-UI) export serialization: CSV / JSON / Markdown, each ending with
//! per-project and per-day totals. Kept gpui-free so it is unit-testable.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::models::{format_hms, ExportRow};

struct Totals {
    by_project: Vec<(String, i64)>,
    by_day: Vec<(String, i64)>,
    grand: i64,
}

impl Totals {
    fn from_rows(rows: &[ExportRow]) -> Self {
        let mut by_project: BTreeMap<String, i64> = BTreeMap::new();
        let mut by_day: BTreeMap<String, i64> = BTreeMap::new();
        let mut grand = 0;
        for r in rows {
            *by_project.entry(r.project.clone()).or_default() += r.duration_secs;
            *by_day.entry(r.date.clone()).or_default() += r.duration_secs;
            grand += r.duration_secs;
        }
        Self {
            by_project: by_project.into_iter().collect(),
            by_day: by_day.into_iter().collect(),
            grand,
        }
    }
}

pub fn write_exports(
    dir: &Path,
    rows: &[ExportRow],
    csv: bool,
    json: bool,
    md: bool,
) -> anyhow::Result<usize> {
    let totals = Totals::from_rows(rows);
    let mut n = 0;
    if csv {
        write_csv(&dir.join("report.csv"), rows, &totals)?;
        n += 1;
    }
    if json {
        write_json(&dir.join("report.json"), rows, &totals)?;
        n += 1;
    }
    if md {
        write_md(&dir.join("report.md"), rows, &totals)?;
        n += 1;
    }
    Ok(n)
}

fn write_csv(path: &Path, rows: &[ExportRow], totals: &Totals) -> anyhow::Result<()> {
    let mut w = csv::Writer::from_path(path)?;
    w.write_record([
        "date", "project", "description", "start", "end", "duration_secs", "duration_hms",
    ])?;
    for r in rows {
        let secs = r.duration_secs.to_string();
        w.write_record([
            r.date.as_str(),
            r.project.as_str(),
            r.description.as_str(),
            r.start.as_str(),
            r.end.as_str(),
            secs.as_str(),
            r.duration_hms.as_str(),
        ])?;
    }
    for (proj, secs) in &totals.by_project {
        let (ss, hh) = (secs.to_string(), format_hms(*secs));
        w.write_record(["TOTAL project", proj.as_str(), "", "", "", ss.as_str(), hh.as_str()])?;
    }
    for (day, secs) in &totals.by_day {
        let (ss, hh) = (secs.to_string(), format_hms(*secs));
        w.write_record(["TOTAL day", day.as_str(), "", "", "", ss.as_str(), hh.as_str()])?;
    }
    let (gs, gh) = (totals.grand.to_string(), format_hms(totals.grand));
    w.write_record(["TOTAL all", "", "", "", "", gs.as_str(), gh.as_str()])?;
    w.flush()?;
    Ok(())
}

#[derive(Serialize)]
struct TotalEntry {
    key: String,
    seconds: i64,
    hms: String,
}

#[derive(Serialize)]
struct JsonDoc<'a> {
    rows: &'a [ExportRow],
    totals_by_project: Vec<TotalEntry>,
    totals_by_day: Vec<TotalEntry>,
    total_seconds: i64,
    total_hms: String,
}

fn write_json(path: &Path, rows: &[ExportRow], totals: &Totals) -> anyhow::Result<()> {
    let to_entries = |v: &[(String, i64)]| -> Vec<TotalEntry> {
        v.iter()
            .map(|(k, s)| TotalEntry {
                key: k.clone(),
                seconds: *s,
                hms: format_hms(*s),
            })
            .collect()
    };
    let doc = JsonDoc {
        rows,
        totals_by_project: to_entries(&totals.by_project),
        totals_by_day: to_entries(&totals.by_day),
        total_seconds: totals.grand,
        total_hms: format_hms(totals.grand),
    };
    let file = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(file, &doc)?;
    Ok(())
}

fn write_md(path: &Path, rows: &[ExportRow], totals: &Totals) -> anyhow::Result<()> {
    let mut s = String::new();
    s.push_str("# Time report\n\n");
    s.push_str("| Date | Project | Description | Start | End | Duration |\n");
    s.push_str("|---|---|---|---|---|---|\n");
    for r in rows {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            r.date,
            r.project,
            r.description.replace('|', "\\|"),
            r.start,
            r.end,
            r.duration_hms,
        ));
    }
    s.push_str("\n## Totals by project\n\n");
    for (p, secs) in &totals.by_project {
        s.push_str(&format!("- {p}: {}\n", format_hms(*secs)));
    }
    s.push_str("\n## Totals by day\n\n");
    for (d, secs) in &totals.by_day {
        s.push_str(&format!("- {d}: {}\n", format_hms(*secs)));
    }
    s.push_str(&format!("\n**Total: {}**\n", format_hms(totals.grand)));
    std::fs::write(path, s)?;
    Ok(())
}
