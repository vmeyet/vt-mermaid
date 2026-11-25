//! One self-contained HTML page: per input its source, then every tool's drawing side by side with its metrics.

use std::collections::BTreeMap;

use crate::metrics::Metrics;
use crate::results::InputResult;
use crate::tool::{Run, Tool};

/// One input as the gallery shows it.
#[derive(Debug)]
pub(crate) struct Entry<'a> {
    pub(crate) source: &'a str,
    pub(crate) result: &'a InputResult,
    pub(crate) runs: BTreeMap<(Tool, u16), &'a Run>,
}

const STYLE: &str = r#"
:root { --bg: #fff; --fg: #1d1d1f; --muted: #6e6e73; --card: #f5f5f7; --line: #d2d2d7; --bad: #b3261e; --good: #1b7f3b; }
@media (prefers-color-scheme: dark) { :root { --bg: #161617; --fg: #f5f5f7; --muted: #a1a1a6; --card: #1f1f21; --line: #3a3a3c; --bad: #ff6b60; --good: #5fd27e; color-scheme: dark; } }
body { background: var(--bg); color: var(--fg); font: 14px/1.4 system-ui, sans-serif; margin: 0; padding: 0 16px 48px; }
header { position: sticky; top: 0; background: var(--bg); padding: 12px 0; border-bottom: 1px solid var(--line); z-index: 1; display: flex; flex-wrap: wrap; gap: 8px 16px; align-items: center; }
h1 { font-size: 18px; margin: 0; }
section { border-bottom: 1px solid var(--line); padding: 16px 0; }
h2 { font-size: 15px; margin: 0 0 8px; font-family: ui-monospace, monospace; }
.gap { color: var(--muted); font-weight: normal; }
.row { display: flex; gap: 12px; overflow-x: auto; align-items: flex-start; }
.card { background: var(--card); border-radius: 6px; padding: 8px; flex: none; max-width: 100%; }
.card h3 { font-size: 13px; margin: 0 0 4px; }
.card p { margin: 0 0 6px; color: var(--muted); font-size: 12px; }
pre { font: 12px/1.15 ui-monospace, "SF Mono", Menlo, monospace; margin: 0; overflow-x: auto; }
.refused, .crashed, .timed_out { color: var(--bad); }
details { margin-bottom: 8px; }
[hidden] { display: none !important; }
"#;

const SCRIPT: &str = r"
const sections = [...document.querySelectorAll('section')];
const list = document.querySelector('main');
let width = document.querySelector('#width').value;
function show() {
  document.querySelectorAll('[data-width]').forEach((card) => { card.hidden = card.dataset.width !== width; });
  sections.forEach((section) => { section.querySelector('.gap').textContent = section.dataset['gap' + width] === '' ? '' : `vt-mermaid − best competitor: ${section.dataset['gap' + width]}`; });
  const order = document.querySelector('#order').value;
  const gap = (section) => { const value = section.dataset['gap' + width]; return value === '' ? Infinity : Number(value); };
  const sorted = [...sections].sort((a, b) => order === 'name' ? a.id.localeCompare(b.id) : order === 'best' ? gap(b) - gap(a) : gap(a) - gap(b));
  sorted.forEach((section) => list.append(section));
}
document.querySelector('#width').addEventListener('change', (event) => { width = event.target.value; show(); });
document.querySelector('#order').addEventListener('change', show);
show();
";

pub(crate) fn page(entries: &[Entry<'_>], widths: &[u16]) -> String {
    let options: String = widths.iter().rev().map(|width| format!("<option value=\"{width}\">{width} columns</option>")).collect();
    let sections: String = entries.iter().map(|entry| section(entry, widths)).collect();
    format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width\">\n<title>vt-mermaid scoreboard</title>\n<style>{STYLE}</style>\n<header><h1>vt-mermaid scoreboard</h1>\
         <label>Width <select id=\"width\">{options}</select></label>\
         <label>Order <select id=\"order\"><option value=\"worst\">vt-mermaid worst vs best competitor</option>\
         <option value=\"best\">vt-mermaid best vs best competitor</option><option value=\"name\">name</option></select></label>\
         <span>{} inputs; score 0–100, see scoreboard/README.md</span></header>\n<main>{sections}</main>\n<script>{SCRIPT}</script>\n",
        entries.len()
    )
}

fn section(entry: &Entry<'_>, widths: &[u16]) -> String {
    let gaps: String = widths
        .iter()
        .map(|&width| format!(" data-gap{width}=\"{}\"", gap(entry.result, width).map_or_else(String::new, |gap| gap.to_string())))
        .collect();
    let cards: String = widths
        .iter()
        .flat_map(|&width| Tool::ALL.into_iter().map(move |tool| (tool, width)))
        .filter_map(|(tool, width)| Some(card(tool, width, entry.result.metrics(tool, width)?, entry.runs.get(&(tool, width)).copied())))
        .collect();
    let name = escaped(&entry.result.input);
    format!(
        "<section id=\"{name}\"{gaps}><h2>{name} <span class=\"gap\"></span></h2>\
         <details><summary>source</summary><pre>{}</pre></details><div class=\"row\">{cards}</div></section>\n",
        escaped(entry.source)
    )
}

/// vt-mermaid's score minus the best competitor's: below zero, a competitor did better.
fn gap(result: &InputResult, width: u16) -> Option<i64> {
    let score = |tool| result.metrics(tool, width).and_then(|metrics| metrics.score).map(i64::from);
    let best = Tool::ALL.into_iter().filter(|&tool| tool != Tool::VtMermaid).filter_map(score).max()?;
    Some(score(Tool::VtMermaid)? - best)
}

fn card(tool: Tool, width: u16, metrics: &Metrics, run: Option<&Run>) -> String {
    let facts = metrics.drawn.map_or_else(String::new, |drawn| {
        format!(
            "{}×{}{}, labels {}+{}, arrows {}, crossings {}, bends {}",
            drawn.width,
            drawn.rows,
            if drawn.fits { "" } else { " (too wide)" },
            drawn.node_labels,
            drawn.edge_labels,
            drawn.arrows,
            drawn.crossings,
            drawn.bends
        )
    });
    let score = metrics.score.map_or_else(String::new, |score| format!("score {score} · "));
    let time = run.map_or_else(String::new, |run| format!(" · {} ms", run.millis));
    let body = match run {
        Some(run) => format!("<pre class=\"{}\">{}</pre>", status_name(metrics), escaped(&run.text)),
        None => "<p>not run this time</p>".to_owned(),
    };
    format!(
        "<div class=\"card\" data-width=\"{width}\"><h3>{}</h3><p>{score}{}{facts}{time}</p>{body}</div>",
        tool.name(),
        if metrics.drawn.is_some() { String::new() } else { format!("{} ", status_name(metrics)) }
    )
}

fn status_name(metrics: &Metrics) -> String {
    serde_json::to_value(metrics.status).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default()
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
