use std::path::PathBuf;
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, Pos2, Rect, RichText, Sense, Stroke, Vec2};

const INK: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x1d);
const MUTED: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);
const LINE: Color32 = Color32::from_rgb(0xe7, 0xe7, 0xe7);
const SOFT: Color32 = Color32::from_rgb(0xf5, 0xf5, 0xf5);
const BAR: Color32 = Color32::from_rgb(0xe4, 0xe4, 0xe4);
const WARN: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9a);

// ---------- dates (days since 1970-01-01, no external crates) ----------
fn days_from_civil(y: i64, m: i64, d: i64) -> f64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) as f64
}
fn civil_from_days(z: f64) -> (i64, i64, i64) {
    let z = z as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
fn fmt(d: f64) -> String { let (_, m, dd) = civil_from_days(d); format!("{} {}", dd, MON[(m - 1) as usize]) }
fn iso(d: f64) -> String { let (y, m, dd) = civil_from_days(d); format!("{:04}-{:02}-{:02}", y, m, dd) }
fn parse_date(s: &str) -> Option<f64> {
    let p: Vec<&str> = s.trim().split('-').collect();
    if p.len() != 3 { return None; }
    let (y, m, d) = (p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    Some(days_from_civil(y, m, d))
}

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut cell, mut q) = (vec![], vec![], String::new(), false);
    let ch: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if q {
            if c == '"' { if i + 1 < ch.len() && ch[i + 1] == '"' { cell.push('"'); i += 1; } else { q = false; } } else { cell.push(c); }
        } else if c == '"' { q = true; }
        else if c == ',' { row.push(std::mem::take(&mut cell)); }
        else if c == '\n' || c == '\r' {
            if c == '\r' && i + 1 < ch.len() && ch[i + 1] == '\n' { i += 1; }
            row.push(std::mem::take(&mut cell));
            if row.iter().any(|x| !x.trim().is_empty()) { rows.push(std::mem::take(&mut row)); } else { row.clear(); }
        } else { cell.push(c); }
        i += 1;
    }
    if !cell.is_empty() || !row.is_empty() { row.push(cell); rows.push(row); }
    rows
}


// ---------- data ----------
#[derive(Clone)]
struct Task { id: String, name: String, start: f64, end: f64, pct: f64, dep: String }
#[derive(Clone)]
struct Project { id: String, name: String, contractor: String, owner: String, budget: f64, spent: f64, rejected: u32, tasks: Vec<Task> }
#[derive(Clone)]
struct Blocker { id: u32, pid: String, task: String, kind: String, text: String, owner: String, due: String, open: bool, opened: String, resolved: String }
#[derive(Clone)]
struct Update { id: u32, pid: String, date: String, kind: String, note: String, evidence: String }
#[derive(Clone)]
struct Plan { pid: String, committed: String, base_slip: i64, steps: String, owner: String, created: String }

const KINDS: [&str; 6] = ["Material", "Clearance or permit", "Design or drawing", "Payment", "Contractor capacity", "Other"];

fn today_iso() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    iso((secs / 86400) as f64)
}

fn parse_projects(text: &str) -> Result<Vec<Project>, String> {
    let rows = parse_csv(text.trim_start_matches('\u{feff}'));
    if rows.is_empty() { return Err("The file is empty.".into()); }
    let h: Vec<String> = rows[0].iter().map(|x| x.trim().to_lowercase()).collect();
    let need = ["project_id", "project", "contractor", "budget_lakh", "spent_lakh", "task_id", "task", "start", "end", "pct_complete"];
    let miss: Vec<&str> = need.iter().filter(|n| !h.iter().any(|x| x == *n)).cloned().collect();
    if !miss.is_empty() { return Err(format!("Missing columns: {}. Optional: rejected_lots, depends_on, owner.", miss.join(", "))); }
    let ix = |n: &str| h.iter().position(|x| x == n);
    let mut out: Vec<Project> = vec![];
    for (k, r) in rows.iter().enumerate().skip(1) {
        let g = |n: &str| ix(n).and_then(|i| r.get(i)).map(|s| s.trim().to_string()).unwrap_or_default();
        let id = g("project_id");
        if id.is_empty() { continue; }
        let (s, e) = (parse_date(&g("start")), parse_date(&g("end")));
        let pc = g("pct_complete").parse::<f64>().ok();
        let (s, e, pc) = match (s, e, pc) { (Some(s), Some(e), Some(p)) if e > s => (s, e, p), _ => return Err(format!("Row {}: check dates (YYYY-MM-DD, end after start) and pct_complete.", k + 1)) };
        let t = Task { id: g("task_id"), name: g("task"), start: s, end: e, pct: (pc / 100.0).clamp(0.0, 1.0), dep: g("depends_on") };
        if let Some(p) = out.iter_mut().find(|p| p.id == id) { p.tasks.push(t); } else {
            out.push(Project { id, name: g("project"), contractor: g("contractor"), owner: g("owner"), budget: g("budget_lakh").parse().unwrap_or(0.0), spent: g("spent_lakh").parse().unwrap_or(0.0), rejected: g("rejected_lots").parse().unwrap_or(0), tasks: vec![t] });
        }
    }
    if out.is_empty() { return Err("No project rows found.".into()); }
    Ok(out)
}

// ---------- store (one per workspace, saved to a plain text file on every change) ----------
fn esc(s: &str) -> String { s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "") }
fn unesc(s: &str) -> String {
    let mut o = String::new(); let mut it = s.chars();
    while let Some(c) = it.next() { if c == '\\' { match it.next() { Some('t') => o.push('\t'), Some('n') => o.push('\n'), Some(x) => o.push(x), None => {} } } else { o.push(c); } }
    o
}

struct Store { projects: Vec<Project>, blockers: Vec<Blocker>, updates: Vec<Update>, plans: Vec<Plan>, asof: String, next: u32, path: PathBuf, sample: bool }

impl Store {
    fn new(path: PathBuf, sample: bool) -> Store {
        Store { projects: vec![], blockers: vec![], updates: vec![], plans: vec![], asof: if sample { "2026-11-11".into() } else { today_iso() }, next: 1, path, sample }
    }
    fn asof_d(&self) -> f64 { parse_date(&self.asof).unwrap_or_else(|| days_from_civil(2026, 11, 11)) }
    fn save(&self) {
        let mut s = String::new();
        s.push_str(&format!("M\t{}\t{}\n", esc(&self.asof), self.next));
        for p in &self.projects {
            s.push_str(&format!("P\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", esc(&p.id), esc(&p.name), esc(&p.contractor), esc(&p.owner), p.budget, p.spent, p.rejected));
            for t in &p.tasks { s.push_str(&format!("T\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", esc(&p.id), esc(&t.id), esc(&t.name), iso(t.start), iso(t.end), t.pct, esc(&t.dep))); }
        }
        for b in &self.blockers { s.push_str(&format!("B\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", b.id, esc(&b.pid), esc(&b.task), esc(&b.kind), esc(&b.text), esc(&b.owner), esc(&b.due), if b.open { 1 } else { 0 }, esc(&b.opened), esc(&b.resolved))); }
        for u in &self.updates { s.push_str(&format!("U\t{}\t{}\t{}\t{}\t{}\t{}\n", u.id, esc(&u.pid), esc(&u.date), esc(&u.kind), esc(&u.note), esc(&u.evidence))); }
        for r in &self.plans { s.push_str(&format!("R\t{}\t{}\t{}\t{}\t{}\t{}\n", esc(&r.pid), esc(&r.committed), r.base_slip, esc(&r.steps), esc(&r.owner), esc(&r.created))); }
        if let Some(d) = self.path.parent() { let _ = std::fs::create_dir_all(d); }
        let tmp = self.path.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() { let _ = std::fs::rename(&tmp, &self.path); }
    }
    fn load(path: PathBuf, sample: bool) -> Option<Store> {
        let text = std::fs::read_to_string(&path).ok()?;
        let mut st = Store::new(path, sample);
        for line in text.lines() {
            let f: Vec<String> = line.split('\t').map(unesc).collect();
            let g = |i: usize| f.get(i).cloned().unwrap_or_default();
            match f[0].as_str() {
                "M" => { st.asof = g(1); st.next = g(2).parse().unwrap_or(1); }
                "P" => st.projects.push(Project { id: g(1), name: g(2), contractor: g(3), owner: g(4), budget: g(5).parse().unwrap_or(0.0), spent: g(6).parse().unwrap_or(0.0), rejected: g(7).parse().unwrap_or(0), tasks: vec![] }),
                "T" => { if let Some(p) = st.projects.iter_mut().find(|p| p.id == g(1)) { p.tasks.push(Task { id: g(2), name: g(3), start: parse_date(&g(4)).unwrap_or(0.0), end: parse_date(&g(5)).unwrap_or(1.0), pct: g(6).parse().unwrap_or(0.0), dep: g(7) }); } }
                "B" => st.blockers.push(Blocker { id: g(1).parse().unwrap_or(0), pid: g(2), task: g(3), kind: g(4), text: g(5), owner: g(6), due: g(7), open: g(8) == "1", opened: g(9), resolved: g(10) }),
                "U" => st.updates.push(Update { id: g(1).parse().unwrap_or(0), pid: g(2), date: g(3), kind: g(4), note: g(5), evidence: g(6) }),
                "R" => st.plans.push(Plan { pid: g(1), committed: g(2), base_slip: g(3).parse().unwrap_or(0), steps: g(4), owner: g(5), created: g(6) }),
                _ => {}
            }
        }
        Some(st)
    }
    fn log(&mut self, pid: &str, kind: &str, note: String, evidence: String) {
        self.next += 1;
        self.updates.push(Update { id: self.next, pid: pid.into(), date: self.asof.clone(), kind: kind.into(), note, evidence });
    }
    fn pname(&self, pid: &str) -> String { self.projects.iter().find(|p| p.id == pid).map(|p| p.name.clone()).unwrap_or_default() }
    fn tname(&self, pid: &str, tid: &str) -> String { self.projects.iter().find(|p| p.id == pid).and_then(|p| p.tasks.iter().find(|t| t.id == tid)).map(|t| t.name.clone()).unwrap_or_else(|| "whole project".into()) }

    fn import(&mut self, text: &str) -> Result<String, String> {
        let incoming = parse_projects(text)?;
        let (mut new, mut upd, mut changes) = (0, 0, 0);
        for ip in incoming {
            if let Some(p) = self.projects.iter().position(|p| p.id == ip.id) {
                upd += 1;
                let mut notes: Vec<String> = vec![];
                {
                    let ex = &mut self.projects[p];
                    if ip.spent != ex.spent { notes.push(format!("spend Rs {} -> {} lakh", ex.spent, ip.spent)); ex.spent = ip.spent; }
                    if ip.rejected != ex.rejected { notes.push(format!("rejected lots {} -> {}", ex.rejected, ip.rejected)); ex.rejected = ip.rejected; }
                    if ip.budget != ex.budget && ip.budget > 0.0 { notes.push(format!("budget {} -> {} lakh", ex.budget, ip.budget)); ex.budget = ip.budget; }
                    for t in ip.tasks {
                        if let Some(et) = ex.tasks.iter_mut().find(|x| x.id == t.id) {
                            if (et.pct - t.pct).abs() > 0.001 { notes.push(format!("{} {:.0}% -> {:.0}%", et.name, et.pct * 100.0, t.pct * 100.0)); et.pct = t.pct; }
                            if et.start != t.start || et.end != t.end { notes.push(format!("{} dates {} to {} -> {} to {}", et.name, iso(et.start), iso(et.end), iso(t.start), iso(t.end))); et.start = t.start; et.end = t.end; }
                        } else { notes.push(format!("task added: {}", t.name)); ex.tasks.push(t); }
                    }
                }
                if !notes.is_empty() { changes += notes.len(); let pid = ip.id.clone(); self.log(&pid, "import", format!("Changed by file import: {}.", notes.join("; ")), "CSV import".into()); }
            } else {
                new += 1; let pid = ip.id.clone(); let n = ip.tasks.len();
                self.projects.push(ip);
                self.log(&pid, "import", format!("Project added by file import with {} tasks.", n), "CSV import".into());
            }
        }
        self.save();
        Ok(format!("Imported: {} new project{}, {} existing updated ({} changes).", new, if new == 1 { "" } else { "s" }, upd, changes))
    }
}

// ---------- rules ----------
#[derive(Clone, Copy, PartialEq)]
enum Status { Risk, Watch, Ok }
#[derive(Clone)]
struct TaskCalc { t: Task, fend: f64, shift: f64, no_progress: bool }
#[derive(Clone)]
struct Calc { p: Project, tasks: Vec<TaskCalc>, planned: f64, actual: f64, spi: Option<f64>, cpi: Option<f64>, planned_end: f64, fend: f64, slip: i64, flags: Vec<String>, status: Status, open_blockers: usize }

fn calc(p: &Project, asof: f64, what: Option<(&str, f64)>, blockers: &[Blocker]) -> Calc {
    let mut tc: Vec<TaskCalc> = vec![];
    for t in &p.tasks {
        let (own, np) = if t.pct >= 1.0 || asof <= t.start { (t.end, false) }
            else if t.pct > 0.0 { (asof + (1.0 - t.pct) * (asof - t.start) / t.pct, false) }
            else { (t.end + (asof - t.start), true) };
        let shift = if t.pct < 1.0 { t.dep.is_empty().then_some(0.0).unwrap_or_else(|| tc.iter().find(|x| x.t.id == t.dep).map(|d| (d.fend - d.t.end).max(0.0)).unwrap_or(0.0)) } else { 0.0 };
        let mut fend = own + shift;
        if let Some((id, slip)) = what { if id == t.id { fend += slip; } }
        tc.push(TaskCalc { t: t.clone(), fend, shift, no_progress: np });
    }
    let dur: f64 = tc.iter().map(|x| x.t.end - x.t.start).sum::<f64>().max(1.0);
    let plan = |x: &TaskCalc| ((asof - x.t.start).clamp(0.0, x.t.end - x.t.start)) / (x.t.end - x.t.start);
    let planned = tc.iter().map(|x| plan(x) * (x.t.end - x.t.start)).sum::<f64>() / dur;
    let actual = tc.iter().map(|x| x.t.pct * (x.t.end - x.t.start)).sum::<f64>() / dur;
    let spi = (planned > 0.02).then(|| actual / planned);
    let cpi = (p.spent > 0.0).then(|| actual * p.budget / p.spent);
    let planned_end = tc.iter().map(|x| x.t.end).fold(f64::MIN, f64::max);
    let fend = tc.iter().map(|x| x.fend).fold(f64::MIN, f64::max);
    let slip = if tc.is_empty() { 0 } else { (fend - planned_end).round() as i64 };
    let mut flags = vec![];
    if let Some(s) = spi { if s < 0.8 { flags.push(format!("Behind schedule: {:.0}% done against {:.0}% planned by {} (ratio {:.2}).", actual * 100.0, planned * 100.0, iso(asof), s)); } }
    if let Some(c) = cpi { if c < 0.9 { flags.push(format!("Cost: Rs {} lakh spent of {}; value earned is {:.0} lakh (ratio {:.2}).", p.spent, p.budget, actual * p.budget, c)); } }
    if slip > 14 { flags.push(format!("Forecast finish {} is {} days after the planned {}.", fmt(fend), slip, fmt(planned_end))); }
    let moved: Vec<String> = tc.iter().filter(|x| x.shift > 14.0).map(|x| format!("\"{}\" moves {} days because its predecessor overruns", x.t.name, x.shift.round())).collect();
    if !moved.is_empty() { flags.push(format!("Downstream effect: {}.", moved.join("; "))); }
    if p.rejected >= 2 { flags.push(format!("Contractor signal: {} rejected material lots recorded.", p.rejected)); }
    for x in tc.iter().filter(|x| x.no_progress) { flags.push(format!("\"{}\" was due to start on {} and reports 0% progress.", x.t.name, fmt(x.t.start))); }
    let mine: Vec<&Blocker> = blockers.iter().filter(|b| b.pid == p.id && b.open).collect();
    let overdue = mine.iter().filter(|b| parse_date(&b.due).map(|d| d < asof).unwrap_or(false)).count();
    if overdue > 0 { flags.push(format!("{} logged blocker{} open past the due date.", overdue, if overdue == 1 { " is" } else { "s are" })); }
    let status = match flags.len() { 0 | 1 => Status::Ok, 2 => Status::Watch, _ => Status::Risk };
    Calc { p: p.clone(), tasks: tc, planned, actual, spi, cpi, planned_end, fend, slip, flags, status, open_blockers: mine.len() }
}

fn label(s: Status) -> &'static str { match s { Status::Risk => "At risk", Status::Watch => "Watch", Status::Ok => "On track" } }
fn mark(painter: &egui::Painter, c: Pos2, s: Status) {
    match s {
        Status::Risk => { painter.circle_filled(c, 5.0, INK); }
        Status::Watch => { painter.circle_stroke(c, 5.0, Stroke::new(1.5, INK)); painter.circle_filled(c, 2.0, INK); }
        Status::Ok => { painter.circle_stroke(c, 5.0, Stroke::new(1.2, WARN)); }
    }
}

fn data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("PROJECT_WATCH_HOME") { return PathBuf::from(p); }
    if let Ok(a) = std::env::var("APPDATA") { return PathBuf::from(a).join("ProjectWatch"); }
    if let Ok(h) = std::env::var("HOME") { return PathBuf::from(h).join(".local/share/project-watch"); }
    PathBuf::from(".")
}
fn sample_store() -> Store {
    let mut s = Store::new(data_dir().join("sample.pw"), true);
    let _ = s.import(include_str!("../sample.csv"));
    s.updates.clear();
    for (i, p) in s.projects.iter_mut().enumerate() { p.owner = ["R. Sharma", "K. Rao", "S. Reddy", "M. Iqbal", "A. Naidu", "V. Kumar", "L. Fernandes", "P. Singh"][i % 8].to_string(); }
    s.save();
    s
}

// ---------- app ----------
#[derive(Clone, Copy, PartialEq)]
enum Tab { Overview, Plan, Blockers, Updates, Recovery }
impl Tab { fn name(self) -> &'static str { match self { Tab::Overview => "Overview", Tab::Plan => "Plan", Tab::Blockers => "Blockers", Tab::Updates => "Updates", Tab::Recovery => "Recovery" } } }

#[derive(Default)]
struct Forms {
    prog_task: String, prog_for: String, prog_pct: f32, prog_note: String, prog_ev: String,
    spend: String, spend_note: String, own: String, rej: String, det_for: String,
    rs_task: String, rs_for: String, rs_start: String, rs_end: String, rs_note: String,
    at_name: String, at_start: String, at_end: String, at_dep: String,
    b_task: String, b_kind: String, b_text: String, b_owner: String, b_due: String, res_note: String,
    pl_date: String, pl_steps: String, pl_owner: String, close_note: String,
    np_name: String, np_contractor: String, np_owner: String, np_budget: String,
}

enum Act {
    Progress(String, String, f64, String, String), Spend(String, f64, String), Details(String, String, u32),
    Resched(String, String, f64, f64, String), AddTask(String, String, f64, f64, String), AddBlocker(Blocker),
    Resolve(u32, String), StartPlan(Plan), ClosePlan(String, String), NewProject(Project),
}

struct App {
    ws: Store, sm: Store, use_sample: bool, asof_buf: String, sel: Option<String>, tab: Tab, actions_view: bool,
    what_task: Option<String>, what_slip: f32, copied: bool, f: Forms, notice: String, new_open: bool, confirm_reset: bool,
}

fn setup(ctx: &egui::Context) {
    let mut f = FontDefinitions::default();
    f.font_data.insert("inter".into(), std::sync::Arc::new(FontData::from_static(include_bytes!("../assets/Inter-Regular.ttf"))));
    f.font_data.insert("inter-semi".into(), std::sync::Arc::new(FontData::from_static(include_bytes!("../assets/Inter-SemiBold.ttf"))));
    f.families.insert(FontFamily::Proportional, vec!["inter".into()]);
    f.families.insert(FontFamily::Name("semi".into()), vec!["inter-semi".into()]);
    ctx.set_fonts(f);
    let mut v = egui::Visuals::light();
    v.panel_fill = Color32::WHITE; v.window_fill = Color32::WHITE; v.extreme_bg_color = Color32::WHITE;
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered] { w.fg_stroke = Stroke::new(1.0, INK); }
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.inactive.bg_fill = SOFT; v.widgets.inactive.weak_bg_fill = SOFT; v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xcf, 0xcf, 0xcf));
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xec, 0xec, 0xec); v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xec, 0xec, 0xec); v.widgets.hovered.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.active.bg_fill = INK; v.widgets.active.weak_bg_fill = INK;
    v.selection.bg_fill = Color32::from_rgb(0xd9, 0xd9, 0xd9); v.selection.stroke = Stroke::new(1.0, INK);
    v.widgets.inactive.corner_radius = 6.0.into(); v.widgets.hovered.corner_radius = 6.0.into(); v.widgets.active.corner_radius = 6.0.into();
    ctx.set_visuals(v);
    ctx.style_mut(|s| { s.spacing.item_spacing = Vec2::new(10.0, 8.0); s.spacing.button_padding = Vec2::new(12.0, 6.0); });
}
fn semi(t: &str, sz: f32) -> RichText { RichText::new(t).size(sz).family(FontFamily::Name("semi".into())) }
fn small(t: &str) -> RichText { RichText::new(t).size(12.0).color(MUTED) }
fn sec(ui: &mut egui::Ui, t: &str) { ui.add_space(14.0); ui.label(small(t)); }
fn edit(ui: &mut egui::Ui, s: &mut String, w: f32, hint: &str) { ui.add(egui::TextEdit::singleline(s).desired_width(w).hint_text(hint)); }
fn primary(ui: &mut egui::Ui, t: &str, enabled: bool) -> bool { ui.add_enabled(enabled, egui::Button::new(RichText::new(t).color(Color32::WHITE)).fill(INK)).clicked() }
fn task_combo(ui: &mut egui::Ui, id: &str, cur: &mut String, tasks: &[(String, String)], none: Option<&str>) {
    if none.is_none() && !tasks.iter().any(|t| t.0 == *cur) { *cur = tasks.first().map(|t| t.0.clone()).unwrap_or_default(); }
    let text = tasks.iter().find(|t| t.0 == *cur).map(|t| t.1.clone()).or(none.map(|s| s.to_string())).unwrap_or_default();
    egui::ComboBox::from_id_salt(id).selected_text(text).show_ui(ui, |ui| {
        if let Some(n) = none { ui.selectable_value(cur, String::new(), n); }
        for t in tasks { ui.selectable_value(cur, t.0.clone(), &t.1); }
    });
}
fn days_over(due: &str, asof: f64) -> Option<i64> { parse_date(due).map(|d| (asof - d).round() as i64).filter(|x| *x > 0) }

fn timeline(ui: &mut egui::Ui, ctx: &egui::Context, c: &Calc, asof: f64) {
    let rows = c.tasks.len() as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), rows * 38.0 + 30.0), Sense::hover());
    let p = ui.painter_at(rect);
    let t0 = c.tasks.iter().map(|x| x.t.start).fold(f64::MAX, f64::min);
    let t1 = c.planned_end.max(c.fend) + 10.0;
    let lx = rect.left() + 150.0; let rx = rect.right() - 40.0;
    let xs = |d: f64| lx + ((d - t0) / (t1 - t0)) as f32 * (rx - lx);
    let (mut y, mut m) = { let (yy, mm, _) = civil_from_days(t0); (yy, mm) };
    loop { m += 1; if m > 12 { m = 1; y += 1; } let d = days_from_civil(y, m, 1); if d > t1 { break; } let x = xs(d); p.line_segment([Pos2::new(x, rect.top() + 16.0), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(0xf0, 0xf0, 0xf0))); p.text(Pos2::new(x + 4.0, rect.top()), egui::Align2::LEFT_TOP, MON[(m - 1) as usize], egui::FontId::proportional(10.5), MUTED); }
    let ease = ctx.animate_value_with_time(egui::Id::new(("tl", &c.p.id)), 1.0, 0.6);
    for (i, x) in c.tasks.iter().enumerate() {
        let top = rect.top() + 28.0 + i as f32 * 38.0;
        p.text(Pos2::new(rect.left(), top + 9.0), egui::Align2::LEFT_CENTER, &x.t.name, egui::FontId::proportional(12.5), INK);
        let (a, b) = (xs(x.t.start), xs(x.t.end));
        p.rect_filled(Rect::from_min_max(Pos2::new(a, top), Pos2::new(b, top + 18.0)), 4.0, BAR);
        p.rect_filled(Rect::from_min_max(Pos2::new(a, top), Pos2::new(a + (b - a) * x.t.pct as f32 * ease, top + 18.0)), 4.0, INK);
        if x.t.pct < 1.0 && x.fend > x.t.end + 2.0 {
            let fe = xs(x.fend);
            p.rect_filled(Rect::from_min_max(Pos2::new(b, top + 6.0), Pos2::new(b + (fe - b) * ease, top + 12.0)), 3.0, WARN);
            p.text(Pos2::new(fe + 8.0, top + 9.0), egui::Align2::LEFT_CENTER, format!("+{} d", (x.fend - x.t.end).round()), egui::FontId::proportional(11.0), MUTED);
        }
        if !x.t.dep.is_empty() { if let Some(j) = c.tasks.iter().position(|d| d.t.id == x.t.dep) { let yy = rect.top() + 28.0 + j as f32 * 38.0 + 9.0; let de = xs(c.tasks[j].t.end); p.line_segment([Pos2::new(de, yy), Pos2::new(de + 8.0, yy)], Stroke::new(1.0, WARN)); p.line_segment([Pos2::new(de + 8.0, yy), Pos2::new(de + 8.0, top + 9.0)], Stroke::new(1.0, WARN)); p.line_segment([Pos2::new(de + 8.0, top + 9.0), Pos2::new(a.max(de + 8.0), top + 9.0)], Stroke::new(1.0, WARN)); } }
    }
    let tx = xs(asof);
    let mut yy = rect.top() + 16.0; while yy < rect.bottom() { p.line_segment([Pos2::new(tx, yy), Pos2::new(tx, (yy + 4.0).min(rect.bottom()))], Stroke::new(1.0, INK)); yy += 8.0; }
    p.text(Pos2::new(tx + 4.0, rect.bottom() - 11.0), egui::Align2::LEFT_TOP, format!("as of {}", fmt(asof)), egui::FontId::proportional(10.5), INK);
}

impl App {
    fn store(&self) -> &Store { if self.use_sample { &self.sm } else { &self.ws } }
    fn store_mut(&mut self) -> &mut Store { if self.use_sample { &mut self.sm } else { &mut self.ws } }
    fn switch(&mut self, sample: bool) { self.use_sample = sample; self.sel = None; self.what_task = None; self.what_slip = 0.0; self.f = Forms::default(); self.asof_buf = self.store().asof.clone(); }
    fn import_text(&mut self, text: &str) {
        self.switch(false);
        self.notice = match self.ws.import(text) { Ok(m) => m, Err(e) => format!("Could not read the file. {}", e) };
    }
    fn apply(&mut self, acts: Vec<Act>) {
        let mut msg = String::new();
        let st = self.store_mut();
        for a in acts {
            match a {
                Act::Progress(pid, tid, pct, note, ev) => {
                    let mut old = None; let mut name = String::new();
                    if let Some(t) = st.projects.iter_mut().find(|p| p.id == pid).and_then(|p| p.tasks.iter_mut().find(|t| t.id == tid)) { old = Some(t.pct); name = t.name.clone(); t.pct = pct; }
                    if let Some(o) = old {
                        let changed = (o - pct).abs() > 0.001;
                        let txt = if changed { format!("{}: {:.0}% -> {:.0}%. {}", name, o * 100.0, pct * 100.0, note) } else { format!("{}: {}", name, note) };
                        st.log(&pid, if changed { "progress" } else { "note" }, txt, ev); msg = "Update recorded.".into();
                    }
                }
                Act::Spend(pid, v, note) => { let mut old = 0.0; if let Some(p) = st.projects.iter_mut().find(|p| p.id == pid) { old = p.spent; p.spent = v; } st.log(&pid, "cost", format!("Spend Rs {} -> {} lakh. {}", old, v, note), String::new()); msg = "Spend updated.".into(); }
                Act::Details(pid, owner, rej) => { if let Some(p) = st.projects.iter_mut().find(|p| p.id == pid) { let (oo, or) = (p.owner.clone(), p.rejected); p.owner = owner.clone(); p.rejected = rej; st.log(&pid, "note", format!("Responsible person {} -> {}; rejected lots {} -> {}.", if oo.is_empty() { "none" } else { &oo }, owner, or, rej), String::new()); msg = "Project details updated.".into(); } }
                Act::Resched(pid, tid, s, e, why) => {
                    let mut name = String::new(); let mut before = (0.0, 0.0);
                    if let Some(t) = st.projects.iter_mut().find(|p| p.id == pid).and_then(|p| p.tasks.iter_mut().find(|t| t.id == tid)) { name = t.name.clone(); before = (t.start, t.end); t.start = s; t.end = e; }
                    st.log(&pid, "rebaseline", format!("{}: planned {} to {} -> {} to {}. {}", name, iso(before.0), iso(before.1), iso(s), iso(e), why), String::new()); msg = "Dates changed and logged.".into();
                }
                Act::AddTask(pid, name, s, e, dep) => {
                    if let Some(p) = st.projects.iter_mut().find(|p| p.id == pid) {
                        let mut n = p.tasks.len() + 1; let mut id = format!("{}-{}", pid, n);
                        while p.tasks.iter().any(|t| t.id == id) { n += 1; id = format!("{}-{}", pid, n); }
                        p.tasks.push(Task { id, name: name.clone(), start: s, end: e, pct: 0.0, dep });
                    }
                    st.log(&pid, "plan", format!("Task added: {} ({} to {}).", name, iso(s), iso(e)), String::new()); msg = "Task added.".into();
                }
                Act::AddBlocker(mut b) => { st.next += 1; b.id = st.next; let pid = b.pid.clone(); let tn = st.tname(&pid, &b.task); st.log(&pid, "blocker", format!("Blocker opened ({}): {} Owner {}, due {}. Task: {}.", b.kind, b.text, b.owner, b.due, tn), String::new()); st.blockers.push(b); msg = "Blocker logged.".into(); }
                Act::Resolve(id, note) => {
                    let mut info = None; let asof = st.asof.clone();
                    if let Some(b) = st.blockers.iter_mut().find(|b| b.id == id) { b.open = false; b.resolved = asof; info = Some((b.pid.clone(), b.text.clone())); }
                    if let Some((pid, text)) = info { st.log(&pid, "blocker", format!("Blocker resolved: {} {}", text, note), String::new()); msg = "Blocker resolved.".into(); }
                }
                Act::StartPlan(p) => { let pid = p.pid.clone(); st.log(&pid, "recovery", format!("Recovery plan started: commit to finish by {}. Owner {}. Forecast slip then: +{} d. Steps: {}", p.committed, p.owner, p.base_slip, p.steps), String::new()); st.plans.retain(|x| x.pid != pid); st.plans.push(p); msg = "Recovery plan saved.".into(); }
                Act::ClosePlan(pid, note) => { st.plans.retain(|x| x.pid != pid); st.log(&pid, "recovery", format!("Recovery plan closed. {}", note), String::new()); msg = "Recovery plan closed.".into(); }
                Act::NewProject(p) => { let pid = p.id.clone(); let nm = p.name.clone(); st.projects.push(p); st.log(&pid, "plan", format!("Project created: {}.", nm), String::new()); msg = "Project created.".into(); }
            }
        }
        st.save();
        if !msg.is_empty() { self.notice = msg; }
    }
    fn report(&self, list: &[Calc], asof: f64) -> String {
        let st = self.store();
        let mut s = format!("Project Watch status report, as of {}\n{}\nRule-based flags for human review; not a validated prediction.\n\n", iso(asof), if st.sample { "SAMPLE DATA: invented projects." } else { "Workspace data entered or imported by the user." });
        for c in list {
            s.push_str(&format!("{} [{}]  {}  owner: {}\n  forecast finish {} ({:+} d vs plan {})\n", c.p.name, label(c.status), c.p.contractor, if c.p.owner.is_empty() { "unassigned" } else { &c.p.owner }, iso(c.fend), c.slip, iso(c.planned_end)));
            for f in &c.flags { s.push_str(&format!("  - {}\n", f)); }
            for b in st.blockers.iter().filter(|b| b.pid == c.p.id && b.open) { s.push_str(&format!("  blocker ({}): {} | owner {} | due {}{}\n", b.kind, b.text, b.owner, b.due, days_over(&b.due, asof).map(|d| format!(" | {} d overdue", d)).unwrap_or_default())); }
            if let Some(pl) = st.plans.iter().find(|p| p.pid == c.p.id) { s.push_str(&format!("  recovery plan: commit {} (owner {}), current forecast {}\n", pl.committed, pl.owner, iso(c.fend))); }
            s.push('\n');
        }
        s
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        for d in dropped {
            if let Some(b) = &d.bytes { let t = String::from_utf8_lossy(b).to_string(); self.import_text(&t); }
            else if let Some(p) = &d.path { if let Ok(t) = std::fs::read_to_string(p) { self.import_text(&t); } }
        }
        // as-of date
        if parse_date(&self.asof_buf).is_some() && self.asof_buf != self.store().asof { let a = self.asof_buf.clone(); let st = self.store_mut(); st.asof = a; st.save(); }
        let asof = self.store().asof_d();
        let blockers = self.store().blockers.clone();
        let updates = self.store().updates.clone();
        let plans = self.store().plans.clone();
        let sample = self.use_sample;
        let mut list: Vec<Calc> = vec![];
        for p in &self.store().projects {
            let w = match (&self.sel, &self.what_task) { (Some(s), Some(t)) if *s == p.id => Some((t.as_str(), self.what_slip as f64)), _ => None };
            list.push(calc(p, asof, w, &blockers));
        }
        let rank = |s: Status| match s { Status::Risk => 0, Status::Watch => 1, Status::Ok => 2 };
        list.sort_by(|a, b| rank(a.status).cmp(&rank(b.status)).then(b.flags.len().cmp(&a.flags.len())).then(b.slip.cmp(&a.slip)));
        if self.sel.as_ref().map(|s| !list.iter().any(|c| &c.p.id == s)).unwrap_or(true) { self.sel = list.first().map(|c| c.p.id.clone()); }
        let mut acts: Vec<Act> = vec![];
        let mut switch_to: Option<bool> = None;

        egui::TopBottomPanel::top("top").frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(20, 12)).stroke(Stroke::new(1.0, LINE))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(semi("Project Watch", 16.0));
                ui.add_space(14.0);
                for (nm, s) in [("My workspace", false), ("Sample", true)] {
                    let on = sample == s;
                    let r = ui.add(egui::Button::new(RichText::new(nm).size(12.5).color(if on { Color32::WHITE } else { INK })).fill(if on { INK } else { Color32::TRANSPARENT }));
                    if r.clicked() && !on { switch_to = Some(s); }
                }
                if sample { ui.label(small("SAMPLE DATA - invented projects")); }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if sample {
                        if ui.button(if self.confirm_reset { "Confirm reset" } else { "Reset sample" }).clicked() {
                            if self.confirm_reset { self.sm = sample_store(); self.sel = None; self.confirm_reset = false; self.notice = "Sample restored to its original invented data.".into(); } else { self.confirm_reset = true; }
                        }
                    }
                    #[cfg(windows)]
                    if ui.button("Import CSV").clicked() { if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).pick_file() { if let Ok(t) = std::fs::read_to_string(&p) { self.import_text(&t); } } }
                    if ui.button("Export report").clicked() {
                        let r = self.report(&list, asof);
                        let path = data_dir().join(format!("report-{}.txt", iso(asof)));
                        self.notice = match std::fs::write(&path, r) { Ok(_) => format!("Report saved: {}", path.display()), Err(e) => format!("Could not save report: {}", e) };
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.asof_buf).desired_width(86.0));
                    ui.label(small("As of"));
                });
            });
        });
        egui::TopBottomPanel::bottom("status").frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(20, 6)).stroke(Stroke::new(1.0, LINE))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let left = if !self.notice.is_empty() { self.notice.clone() } else if sample { "SAMPLE DATA: invented projects. Not real utility data.".to_string() } else { format!("Saved on this computer: {}", self.ws.path.display()) };
                ui.label(small(&left));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(small("Rules, not a validated prediction. SAP is not connected. Single user, no sync.")); });
            });
        });

        if list.is_empty() {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add_space(110.0);
                ui.vertical_centered(|ui| {
                    ui.label(semi("Start your project workspace", 22.0));
                    ui.label(small("Drop a project CSV here, or create a project and add tasks yourself."));
                    ui.label(small("CSV columns: project_id, project, contractor, budget_lakh, spent_lakh, task_id, task, start, end, pct_complete. Optional: owner, rejected_lots, depends_on."));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| { ui.add_space((ui.available_width() / 2.0 - 120.0).max(0.0)); if primary(ui, "New project", true) { self.new_open = true; } if ui.button("Open the sample").clicked() { switch_to = Some(true); } });
                });
            });
        } else {
            egui::SidePanel::left("list").exact_width(400.0).frame(egui::Frame::new().fill(Color32::WHITE).stroke(Stroke::new(1.0, LINE))).show(ctx, |ui| {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(20.0);
                    for (nm, a) in [("Projects", false), ("Actions", true)] {
                        let on = self.actions_view == a;
                        if ui.add(egui::Button::new(if on { semi(nm, 13.0) } else { RichText::new(nm).size(13.0).color(MUTED) }).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() { self.actions_view = a; }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.add_space(16.0); if ui.button("New project").clicked() { self.new_open = true; } });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if !self.actions_view {
                        for c in &list {
                            let selected = self.sel.as_deref() == Some(&c.p.id);
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), Sense::click());
                            let t = ctx.animate_bool_with_time(resp.id, selected || resp.hovered(), 0.15);
                            let g = 255 - (10.0 * t) as u8;
                            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(g, g, g));
                            if selected { ui.painter().rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), 0.0, INK); }
                            ui.painter().line_segment([Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())], Stroke::new(1.0, LINE));
                            mark(ui.painter(), Pos2::new(rect.left() + 24.0, rect.center().y), c.status);
                            ui.painter().text(Pos2::new(rect.left() + 42.0, rect.top() + 14.0), egui::Align2::LEFT_TOP, &c.p.name, egui::FontId::new(13.5, FontFamily::Name("semi".into())), INK);
                            let bl = if c.open_blockers > 0 { format!("  ·  {} open blocker{}", c.open_blockers, if c.open_blockers == 1 { "" } else { "s" }) } else { String::new() };
                            ui.painter().text(Pos2::new(rect.left() + 42.0, rect.top() + 36.0), egui::Align2::LEFT_TOP, format!("{}  ·  {} flag{}{}", if c.p.owner.is_empty() { &c.p.contractor } else { &c.p.owner }, c.flags.len(), if c.flags.len() == 1 { "" } else { "s" }, bl), egui::FontId::proportional(12.0), MUTED);
                            let r = if c.slip > 0 { format!("{}  +{} d", label(c.status), c.slip) } else { label(c.status).to_string() };
                            ui.painter().text(Pos2::new(rect.right() - 20.0, rect.center().y), egui::Align2::RIGHT_CENTER, r, egui::FontId::proportional(12.0), if c.status == Status::Ok { MUTED } else { INK });
                            if resp.clicked() { self.sel = Some(c.p.id.clone()); self.what_task = None; self.what_slip = 0.0; self.f = Forms::default(); }
                        }
                    } else {
                        let mut open: Vec<&Blocker> = blockers.iter().filter(|b| b.open).collect();
                        open.sort_by(|a, b| parse_date(&a.due).unwrap_or(1e9).partial_cmp(&parse_date(&b.due).unwrap_or(1e9)).unwrap());
                        if open.is_empty() { ui.add_space(20.0); ui.horizontal(|ui| { ui.add_space(20.0); ui.label(small("No open blockers. Log one from a project's Blockers tab.")); }); }
                        for b in open {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::click());
                            if resp.hovered() { ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(0xf5, 0xf5, 0xf5)); }
                            ui.painter().line_segment([Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())], Stroke::new(1.0, LINE));
                            let pn = self.store().pname(&b.pid);
                            ui.painter().text(Pos2::new(rect.left() + 20.0, rect.top() + 12.0), egui::Align2::LEFT_TOP, &b.text, egui::FontId::new(13.0, FontFamily::Name("semi".into())), INK);
                            ui.painter().text(Pos2::new(rect.left() + 20.0, rect.top() + 33.0), egui::Align2::LEFT_TOP, format!("{}  ·  {}", pn, b.kind), egui::FontId::proportional(12.0), MUTED);
                            let od = days_over(&b.due, asof).map(|d| format!("  ·  {} d overdue", d)).unwrap_or_default();
                            ui.painter().text(Pos2::new(rect.left() + 20.0, rect.top() + 50.0), egui::Align2::LEFT_TOP, format!("{}  ·  due {}{}", b.owner, b.due, od), egui::FontId::proportional(12.0), if od.is_empty() { MUTED } else { INK });
                            if resp.clicked() { self.sel = Some(b.pid.clone()); self.tab = Tab::Blockers; self.actions_view = false; self.f = Forms::default(); }
                        }
                    }
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(32, 22))).show(ctx, |ui| {
                let Some(c) = list.iter().find(|c| Some(&c.p.id) == self.sel.as_ref()) else { return };
                let pid = c.p.id.clone();
                let tasks: Vec<(String, String)> = c.p.tasks.iter().map(|t| (t.id.clone(), t.name.clone())).collect();
                let open_tasks: Vec<(String, String)> = c.p.tasks.iter().filter(|t| t.pct < 1.0).map(|t| (t.id.clone(), t.name.clone())).collect();
                let mine_b: Vec<&Blocker> = blockers.iter().filter(|b| b.pid == pid).collect();
                let plan = plans.iter().find(|p| p.pid == pid);
                ui.horizontal(|ui| { let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 22.0), Sense::hover()); mark(ui.painter(), r.center(), c.status); ui.label(semi(&c.p.name, 20.0)); ui.label(small(label(c.status))); });
                ui.label(small(&format!("{}  ·  responsible: {}  ·  {}  ·  budget Rs {} lakh  ·  spent Rs {} lakh", c.p.contractor, if c.p.owner.is_empty() { "not assigned" } else { &c.p.owner }, c.p.id, c.p.budget, c.p.spent)));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    for t in [Tab::Overview, Tab::Plan, Tab::Blockers, Tab::Updates, Tab::Recovery] {
                        let on = self.tab == t;
                        let n = match t { Tab::Blockers if c.open_blockers > 0 => format!("{} ({})", t.name(), c.open_blockers), _ => t.name().to_string() };
                        let r = ui.add(egui::Button::new(if on { semi(&n, 13.5) } else { RichText::new(n).size(13.5).color(MUTED) }).fill(Color32::TRANSPARENT).stroke(Stroke::NONE));
                        if on { let b = r.rect; ui.painter().line_segment([Pos2::new(b.left() + 8.0, b.bottom() + 1.0), Pos2::new(b.right() - 8.0, b.bottom() + 1.0)], Stroke::new(2.0, INK)); }
                        if r.clicked() { self.tab = t; }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.tab {
                        Tab::Overview => {
                            ui.horizontal(|ui| {
                                let kp = |ui: &mut egui::Ui, v: String, l: &str| { ui.vertical(|ui| { ui.set_width(150.0); ui.label(semi(&v, 22.0)); ui.label(small(l)); }); };
                                kp(ui, format!("{:.0}% / {:.0}%", c.actual * 100.0, c.planned * 100.0), "done / planned today");
                                kp(ui, c.spi.map(|x| format!("{:.2}", x)).unwrap_or("-".into()), "schedule ratio");
                                kp(ui, c.cpi.map(|x| format!("{:.2}", x)).unwrap_or("-".into()), "cost ratio");
                                kp(ui, if c.slip > 0 { format!("+{} d", c.slip) } else { "0 d".into() }, &format!("forecast finish {}", fmt(c.fend)));
                            });
                            if c.tasks.is_empty() { sec(ui, "NO TASKS YET"); ui.label("Add tasks on the Plan tab to see the timeline and forecast."); } else {
                                sec(ui, "PLANNED AND ACTUAL");
                                timeline(ui, ctx, c, asof);
                                ui.label(small("Dark fill is reported progress. Grey is forecast overrun at the current pace. Thin lines show dependencies."));
                            }
                            sec(ui, &format!("WHY IT IS {}", label(c.status).to_uppercase()));
                            if c.flags.is_empty() { ui.label("No rule fired. A person should still read the progress reports."); }
                            for f in &c.flags { ui.horizontal_wrapped(|ui| { ui.label("•"); ui.label(f); }); }
                            if let Some(pl) = plan { let behind = parse_date(&pl.committed).map(|d| c.fend - d).unwrap_or(0.0).round() as i64; sec(ui, "RECOVERY PLAN"); ui.label(format!("Committed finish {}. Current forecast {}. {}", pl.committed, iso(c.fend), if behind > 0 { format!("{} days behind the commitment.", behind) } else { "Within the commitment.".into() })); }
                            if !open_tasks.is_empty() {
                                sec(ui, "WHAT IF A TASK SLIPS (NOT SAVED)");
                                let mut wt = self.what_task.clone().unwrap_or_default();
                                ui.horizontal(|ui| {
                                    task_combo(ui, "wt", &mut wt, &open_tasks, None);
                                    ui.add(egui::Slider::new(&mut self.what_slip, 0.0..=60.0).suffix(" days extra").integer());
                                });
                                self.what_task = Some(wt);
                                ui.label(small(&format!("Project forecast finish: {} ({}{} d against plan). A slip moves the task and anything that depends on it.", fmt(c.fend), if c.slip > 0 { "+" } else { "" }, c.slip)));
                            }
                            sec(ui, "FOLLOW-UP DRAFT (NOT SENT)");
                            let ob: Vec<String> = mine_b.iter().filter(|b| b.open).map(|b| format!("- Open item ({}): {} Owner {}, due {}.", b.kind, b.text, b.owner, b.due)).collect();
                            let draft = if c.flags.is_empty() && ob.is_empty() { "No follow-up suggested by the rules for this project.".to_string() } else if c.status == Status::Ok {
                                format!("To: {}\nCc: {}\nSubject: Quick check - {}\n\nA short note, no escalation. We noticed:\n{}\n{}\nA brief reply with the position is enough.", c.p.contractor, if c.p.owner.is_empty() { "-" } else { &c.p.owner }, c.p.name, c.flags.iter().map(|f| format!("- {}", f)).collect::<Vec<_>>().join("\n"), ob.join("\n"))
                            } else {
                                format!("To: {}\nCc: {}\nSubject: Recovery plan request - {}\n\nPlease share a recovery plan for {}. Points from the status review (as of {}):\n{}\n{}\n\nPlease confirm revised dates, material needs and any support needed from the utility.", c.p.contractor, if c.p.owner.is_empty() { "-" } else { &c.p.owner }, c.p.name, c.p.name, iso(asof), c.flags.iter().map(|f| format!("- {}", f)).collect::<Vec<_>>().join("\n"), ob.join("\n"))
                            };
                            egui::Frame::new().fill(SOFT).corner_radius(6.0).inner_margin(12.0).show(ui, |ui| { ui.set_width(ui.available_width()); ui.label(RichText::new(&draft).size(12.5)); });
                            if ui.button(if self.copied { "Copied" } else { "Copy draft" }).clicked() { ctx.copy_text(draft.clone()); self.copied = true; }
                        }
                        Tab::Plan => {
                            sec(ui, "TASKS");
                            if c.tasks.is_empty() { ui.label(small("No tasks yet. Add the first one below.")); }
                            for x in &c.tasks {
                                ui.horizontal(|ui| { ui.set_min_width(560.0); ui.label(semi(&x.t.name, 13.0)); ui.label(small(&format!("{} to {}  ·  {:.0}%  ·  forecast {}{}", iso(x.t.start), iso(x.t.end), x.t.pct * 100.0, fmt(x.fend), if x.t.dep.is_empty() { String::new() } else { format!("  ·  after {}", tasks.iter().find(|t| t.0 == x.t.dep).map(|t| t.1.clone()).unwrap_or(x.t.dep.clone())) }))); });
                            }
                            if !tasks.is_empty() {
                                sec(ui, "CHANGE PLANNED DATES (RECORDED AS A REBASELINE)");
                                if !tasks.iter().any(|t| t.0 == self.f.rs_task) { self.f.rs_task = tasks[0].0.clone(); }
                                if self.f.rs_for != self.f.rs_task { if let Some(t) = c.p.tasks.iter().find(|t| t.id == self.f.rs_task) { self.f.rs_for = t.id.clone(); self.f.rs_start = iso(t.start); self.f.rs_end = iso(t.end); } }
                                ui.horizontal(|ui| { task_combo(ui, "rs1", &mut self.f.rs_task, &tasks, None); edit(ui, &mut self.f.rs_start, 90.0, "start"); edit(ui, &mut self.f.rs_end, 90.0, "end"); });
                                ui.horizontal(|ui| { edit(ui, &mut self.f.rs_note, 330.0, "reason, e.g. design revised"); });
                                let (ds, de) = (parse_date(&self.f.rs_start), parse_date(&self.f.rs_end));
                                let ok = matches!((ds, de), (Some(a), Some(b)) if b > a) && !self.f.rs_note.trim().is_empty();
                                if primary(ui, "Apply new dates", ok) { acts.push(Act::Resched(pid.clone(), self.f.rs_task.clone(), ds.unwrap(), de.unwrap(), self.f.rs_note.trim().to_string())); self.f.rs_note.clear(); }
                                if !ok { ui.label(small("Dates as YYYY-MM-DD, end after start, and a reason is required.")); }
                            }
                            sec(ui, "ADD A TASK");
                            ui.horizontal(|ui| { edit(ui, &mut self.f.at_name, 180.0, "task name"); edit(ui, &mut self.f.at_start, 90.0, "start"); edit(ui, &mut self.f.at_end, 90.0, "end"); task_combo(ui, "atd", &mut self.f.at_dep, &tasks, Some("no predecessor")); });
                            let (ds, de) = (parse_date(&self.f.at_start), parse_date(&self.f.at_end));
                            if primary(ui, "Add task", matches!((ds, de), (Some(a), Some(b)) if b > a) && !self.f.at_name.trim().is_empty()) { acts.push(Act::AddTask(pid.clone(), self.f.at_name.trim().to_string(), ds.unwrap(), de.unwrap(), self.f.at_dep.clone())); self.f.at_name.clear(); self.f.at_start.clear(); self.f.at_end.clear(); self.f.at_dep.clear(); }
                            sec(ui, "RESPONSIBLE PERSON, REJECTED LOTS, SPEND");
                            if self.f.det_for != pid { self.f.det_for = pid.clone(); self.f.own = c.p.owner.clone(); self.f.rej = c.p.rejected.to_string(); self.f.spend = format!("{}", c.p.spent); }
                            ui.horizontal(|ui| { ui.label(small("Responsible")); edit(ui, &mut self.f.own, 150.0, "name"); ui.label(small("Rejected lots")); edit(ui, &mut self.f.rej, 40.0, "0"); });
                            if ui.button("Save").clicked() { acts.push(Act::Details(pid.clone(), self.f.own.trim().to_string(), self.f.rej.trim().parse().unwrap_or(c.p.rejected))); }
                            ui.horizontal(|ui| { ui.label(small("Spent (Rs lakh)")); edit(ui, &mut self.f.spend, 70.0, "0"); edit(ui, &mut self.f.spend_note, 240.0, "source, e.g. RA bill 4"); });
                            if ui.button("Update spend").clicked() { if let Ok(v) = self.f.spend.trim().parse::<f64>() { acts.push(Act::Spend(pid.clone(), v, self.f.spend_note.trim().to_string())); self.f.spend_note.clear(); } else { self.notice = "Spend must be a number.".into(); } }
                        }
                        Tab::Blockers => {
                            sec(ui, "OPEN");
                            let open: Vec<&&Blocker> = mine_b.iter().filter(|b| b.open).collect();
                            if open.is_empty() { ui.label(small("Nothing open for this project.")); }
                            if !open.is_empty() { ui.horizontal(|ui| { ui.label(small("Resolution note")); edit(ui, &mut self.f.res_note, 320.0, "what fixed it, e.g. fresh lot accepted"); }); }
                            for b in open {
                                egui::Frame::new().stroke(Stroke::new(1.0, LINE)).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(semi(&b.text, 13.0));
                                            let od = days_over(&b.due, asof).map(|d| format!("  ·  {} d overdue", d)).unwrap_or_default();
                                            ui.label(small(&format!("{}  ·  task: {}  ·  owner {}  ·  due {}{}  ·  opened {}", b.kind, self.store().tname(&pid, &b.task), b.owner, b.due, od, b.opened)));
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button("Resolve").clicked() { acts.push(Act::Resolve(b.id, self.f.res_note.trim().to_string())); self.f.res_note.clear(); } });
                                    });
                                });
                            }
                            sec(ui, "LOG A BLOCKER");
                            ui.horizontal(|ui| {
                                task_combo(ui, "bt", &mut self.f.b_task, &tasks, Some("whole project"));
                                if self.f.b_kind.is_empty() { self.f.b_kind = KINDS[0].to_string(); }
                                egui::ComboBox::from_id_salt("bk").selected_text(self.f.b_kind.clone()).show_ui(ui, |ui| { for k in KINDS { ui.selectable_value(&mut self.f.b_kind, k.to_string(), k); } });
                            });
                            ui.horizontal(|ui| { edit(ui, &mut self.f.b_text, 330.0, "what is blocking the work"); });
                            ui.horizontal(|ui| { edit(ui, &mut self.f.b_owner, 150.0, "who clears it"); edit(ui, &mut self.f.b_due, 100.0, "due YYYY-MM-DD"); });
                            let ok = !self.f.b_text.trim().is_empty() && !self.f.b_owner.trim().is_empty() && parse_date(&self.f.b_due).is_some();
                            if primary(ui, "Log blocker", ok) { acts.push(Act::AddBlocker(Blocker { id: 0, pid: pid.clone(), task: self.f.b_task.clone(), kind: self.f.b_kind.clone(), text: self.f.b_text.trim().to_string(), owner: self.f.b_owner.trim().to_string(), due: self.f.b_due.trim().to_string(), open: true, opened: iso(asof), resolved: String::new() })); self.f.b_text.clear(); self.f.b_owner.clear(); self.f.b_due.clear(); }
                            if !ok { ui.label(small("A description, a named owner and a due date are required, so every blocker has someone accountable.")); }
                            let done: Vec<&&Blocker> = mine_b.iter().filter(|b| !b.open).collect();
                            if !done.is_empty() { sec(ui, "RESOLVED"); for b in done { ui.label(small(&format!("{}  ·  {}  ·  owner {}  ·  resolved {}", b.kind, b.text, b.owner, b.resolved))); } }
                        }
                        Tab::Updates => {
                            sec(ui, "RECORD AN UPDATE");
                            if !tasks.is_empty() && !tasks.iter().any(|t| t.0 == self.f.prog_task) { self.f.prog_task = tasks[0].0.clone(); }
                            if self.f.prog_for != self.f.prog_task { if let Some(t) = c.p.tasks.iter().find(|t| t.id == self.f.prog_task) { self.f.prog_for = t.id.clone(); self.f.prog_pct = (t.pct * 100.0).round() as f32; } }
                            let curpct = c.p.tasks.iter().find(|t| t.id == self.f.prog_task).map(|t| t.pct * 100.0).unwrap_or(0.0);
                            ui.horizontal(|ui| { task_combo(ui, "pt", &mut self.f.prog_task, &tasks, None); ui.add(egui::Slider::new(&mut self.f.prog_pct, 0.0..=100.0).suffix(" %").integer()); });
                            ui.horizontal(|ui| { edit(ui, &mut self.f.prog_note, 330.0, "what happened"); });
                            ui.horizontal(|ui| { edit(ui, &mut self.f.prog_ev, 330.0, "evidence, e.g. site photo IMG_2031, MB entry 14"); });
                            let changed = (self.f.prog_pct as f64 - curpct).abs() > 0.5;
                            let ok = !tasks.is_empty() && (!changed || !self.f.prog_ev.trim().is_empty()) && (changed || !self.f.prog_note.trim().is_empty());
                            if primary(ui, if changed { "Record progress" } else { "Record note" }, ok) { acts.push(Act::Progress(pid.clone(), self.f.prog_task.clone(), (self.f.prog_pct as f64 / 100.0).clamp(0.0, 1.0), self.f.prog_note.trim().to_string(), self.f.prog_ev.trim().to_string())); self.f.prog_note.clear(); self.f.prog_ev.clear(); self.f.prog_for.clear(); }
                            if changed && self.f.prog_ev.trim().is_empty() { ui.label(small("A change in percentage needs an evidence reference.")); }
                            sec(ui, "HISTORY");
                            let mine: Vec<&Update> = updates.iter().filter(|u| u.pid == pid).collect();
                            if mine.is_empty() { ui.label(small("No history yet.")); }
                            for u in mine.iter().rev() {
                                ui.horizontal_wrapped(|ui| { ui.label(small(&format!("{}  {:<10}", u.date, u.kind))); ui.label(&u.note); if !u.evidence.is_empty() { ui.label(small(&format!("[evidence: {}]", u.evidence))); } });
                            }
                        }
                        Tab::Recovery => {
                            if let Some(pl) = plan {
                                let behind = parse_date(&pl.committed).map(|d| c.fend - d).unwrap_or(0.0).round() as i64;
                                sec(ui, "ACTIVE PLAN");
                                ui.horizontal(|ui| {
                                    for (v, l) in [(pl.committed.clone(), "committed finish"), (iso(c.fend), "current forecast"), (format!("+{} d", pl.base_slip), "slip when plan began"), (format!("{:+} d", c.slip), "slip now")] { ui.vertical(|ui| { ui.set_width(150.0); ui.label(semi(&v, 20.0)); ui.label(small(l)); }); }
                                });
                                ui.add_space(8.0);
                                ui.label(if behind > 0 { format!("Not on course: the forecast is {} days after the committed finish.", behind) } else { "On course: the forecast is within the committed finish.".to_string() });
                                ui.label(small(&format!("Owner {}  ·  started {}  ·  {} open blocker{} may affect it", pl.owner, pl.created, c.open_blockers, if c.open_blockers == 1 { "" } else { "s" })));
                                sec(ui, "STEPS AGREED");
                                egui::Frame::new().fill(SOFT).corner_radius(6.0).inner_margin(12.0).show(ui, |ui| { ui.set_width(ui.available_width()); ui.label(RichText::new(&pl.steps).size(12.5)); });
                                ui.label(small("The forecast uses the progress recorded on the Updates tab. Record progress and resolve blockers to see whether the plan is working."));
                                sec(ui, "CLOSE THE PLAN");
                                ui.horizontal(|ui| { edit(ui, &mut self.f.close_note, 330.0, "outcome, e.g. back on track after lot re-supply"); });
                                if ui.button("Close plan").clicked() { acts.push(Act::ClosePlan(pid.clone(), self.f.close_note.trim().to_string())); self.f.close_note.clear(); }
                            } else {
                                sec(ui, "START A RECOVERY PLAN");
                                ui.label(format!("Current forecast {} ({:+} d against the planned {}).", iso(c.fend), c.slip, iso(c.planned_end)));
                                ui.label(small("Agree a finish date with the contractor, name who owns the plan and write the steps. The tool then tracks the forecast against that commitment."));
                                ui.add_space(6.0);
                                ui.horizontal(|ui| { edit(ui, &mut self.f.pl_date, 130.0, "committed finish YYYY-MM-DD"); edit(ui, &mut self.f.pl_owner, 170.0, "plan owner"); });
                                ui.add(egui::TextEdit::multiline(&mut self.f.pl_steps).desired_rows(4).desired_width(520.0).hint_text("steps, one per line"));
                                let ok = parse_date(&self.f.pl_date).is_some() && !self.f.pl_owner.trim().is_empty() && !self.f.pl_steps.trim().is_empty();
                                if primary(ui, "Start plan", ok) { acts.push(Act::StartPlan(Plan { pid: pid.clone(), committed: self.f.pl_date.trim().to_string(), base_slip: c.slip.max(0), steps: self.f.pl_steps.trim().to_string(), owner: self.f.pl_owner.trim().to_string(), created: iso(asof) })); self.f.pl_steps.clear(); self.f.pl_date.clear(); self.f.pl_owner.clear(); }
                            }
                        }
                    }
                });
            });
        }
        // new project window
        if self.new_open {
            let mut open = true;
            egui::Window::new("New project").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).open(&mut open).show(ctx, |ui| {
                edit(ui, &mut self.f.np_name, 300.0, "project name");
                edit(ui, &mut self.f.np_contractor, 300.0, "contractor");
                edit(ui, &mut self.f.np_owner, 300.0, "responsible person (utility)");
                edit(ui, &mut self.f.np_budget, 300.0, "budget, Rs lakh");
                let ok = !self.f.np_name.trim().is_empty() && self.f.np_budget.trim().parse::<f64>().is_ok();
                if primary(ui, "Create project", ok) {
                    let n = self.store().projects.len() + 1; let mut id = format!("W{:03}", n);
                    while self.store().projects.iter().any(|p| p.id == id) { id.push('x'); }
                    acts.push(Act::NewProject(Project { id: id.clone(), name: self.f.np_name.trim().into(), contractor: self.f.np_contractor.trim().into(), owner: self.f.np_owner.trim().into(), budget: self.f.np_budget.trim().parse().unwrap_or(0.0), spent: 0.0, rejected: 0, tasks: vec![] }));
                    self.sel = Some(id); self.tab = Tab::Plan; self.new_open = false; self.f.np_name.clear(); self.f.np_contractor.clear(); self.f.np_owner.clear(); self.f.np_budget.clear();
                }
            });
            if !open { self.new_open = false; }
        }
        if let Some(s) = switch_to { self.switch(s); }
        if !acts.is_empty() { self.apply(acts); }
        if self.confirm_reset && !self.use_sample { self.confirm_reset = false; }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let opts = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]).with_min_inner_size([980.0, 600.0]).with_title("Project Watch"), ..Default::default() };
    eframe::run_native("Project Watch", opts, Box::new(move |cc| {
        setup(&cc.egui_ctx);
        let dir = data_dir();
        let ws = Store::load(dir.join("workspace.pw"), false).unwrap_or_else(|| Store::new(dir.join("workspace.pw"), false));
        let sm = Store::load(dir.join("sample.pw"), true).unwrap_or_else(sample_store);
        let mut app = App { asof_buf: ws.asof.clone(), ws, sm, use_sample: false, sel: None, tab: Tab::Overview, actions_view: false, what_task: None, what_slip: 0.0, copied: false, f: Forms::default(), notice: String::new(), new_open: false, confirm_reset: false };
        let mut sample_flag = false;
        for a in args.iter().skip(1) { if a == "--sample" { sample_flag = true; } else if let Ok(t) = std::fs::read_to_string(a) { app.import_text(&t); } }
        if sample_flag { app.switch(true); }
        Ok(Box::new(app))
    }))
}
