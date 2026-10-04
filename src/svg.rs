//! Ok, some SVG drawing helpers :D
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadState {
    /// No Samples Assigned
    Empty,

    /// Samples Assigned, not playing or recording
    Idle,

    /// Actively playing a Sample
    Playing,

    /// Actively Recording a Sample
    Recording,

    /// Clear Samples is Armed!
    ClearArmed,
}

pub struct PadView {
    /// The current state of the action
    pub state: PadState,

    /// The Actions Name
    pub name: String,

    /// The Action's Badge
    pub badge: String,

    /// The number of Samples on this Action
    pub count: usize,

    /// The waveform of the current Active sample
    pub wave: Option<Vec<f64>>,

    /// Trim region of the shown track, percent.
    pub trim: (f64, f64),

    /// 0..=1 while playing.
    pub progress: Option<f64>,

    /// Track Length
    pub secs: f64,

    /// Whether this track should be Blinking
    pub blink: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Glyph {
    Stop,
    Clear,
    Record,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IconKind {
    Idle,
    Active,
    Armed,
    Recording,
}

pub struct IconView {
    pub glyph: Glyph,
    pub label: String,
    pub sub: String,
    pub kind: IconKind,
    pub blink: bool,
}

pub fn data_url(svg: &str) -> String {
    format!("data:image/svg+xml;base64,{}", STANDARD.encode(svg))
}

const WHITE: &str = "#ffffff";
const RED: &str = "#ee4848";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn text(x: f64, y: f64, size: f64, fill: &str, opacity: f64, anchor: &str, s: &str) -> String {
    format!(
        r#"<text x="{x}" y="{y}" font-size="{size}" fill="{fill}" fill-opacity="{opacity}" text-anchor="{anchor}">{}</text>"#,
        esc(s)
    )
}

fn frame(bg: &str, accent: &str, body: &str) -> String {
    format!(
        r##"
            <svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144" font-family="DejaVu Sans, Verdana, Arial, sans-serif" font-weight="700">
                <rect width="144" height="144" rx="22" fill="{bg}" />
                <rect x="2" y="2" width="140" height="140" rx="20" fill="none" stroke="{accent}" stroke-opacity=".3" stroke-width="2" />
                {body}
            </svg>
        "##
    )
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        s.to_string()
    }
}

/// A name over at most two lines of at most 11 characters each.
fn split_name(name: &str) -> Vec<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.len() {
        0 => vec![],
        1 => vec![trunc(words[0], 11)],
        _ => {
            let total = name.chars().count();
            let mut first = String::new();
            let mut i = 0;
            while i < words.len() - 1
                && (first.is_empty()
                    || first.chars().count() + words[i].chars().count() <= total / 2 + 2)
            {
                if !first.is_empty() {
                    first.push(' ');
                }
                first.push_str(words[i]);
                i += 1;
            }
            vec![trunc(&first, 11), trunc(&words[i..].join(" "), 11)]
        }
    }
}

/// The name, centred on `cy`, sized so the longest line fits the key.
fn name_block(name: &str, cy: f64, max: f64) -> String {
    let lines = split_name(name);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f64;
    let size = (118.0 / (longest * 0.62)).clamp(12.0, max);
    match lines.as_slice() {
        [] => String::new(),
        [one] => text(72.0, cy + size * 0.35, size, WHITE, 1.0, "middle", one),
        [a, b, ..] => {
            text(72.0, cy - size * 0.2, size, WHITE, 1.0, "middle", a)
                + &text(72.0, cy + size * 0.85, size, WHITE, 1.0, "middle", b)
        }
    }
}

fn wave(peaks: &[f64], trim: (f64, f64), progress: Option<f64>, color: &str) -> String {
    let (x, y, w, h) = (12.0_f64, 96.0_f64, 120.0_f64, 36.0_f64);
    let n = peaks.len().max(1);
    let bar = w / n as f64;
    let mut out = String::new();
    for i in 0..n {
        let amp = peaks
            .get(i)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
            .powf(0.6)
            .max(0.07);
        let bh = amp * h;
        let pos = (i as f64 + 0.5) / n as f64 * 100.0;
        let opacity = if pos >= trim.0 && pos <= trim.1 {
            1.0
        } else {
            0.24
        };
        out += &format!(
            r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" rx="1" fill="{color}" fill-opacity="{opacity}"/>"#,
            x + i as f64 * bar + 0.8,
            y + (h - bh) / 2.0,
            (bar - 1.6).max(1.0),
            bh
        );
    }
    if let Some(p) = progress {
        out += &format!(
            r##"<rect x="{:.2}" y="{}" width="2.4" height="{}" fill="#ffffff"/>"##,
            x + p.clamp(0.0, 1.0) * w - 1.2,
            y - 4.0,
            h + 8.0
        );
    }
    out
}

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn pad(v: &PadView) -> String {
    let (bg, accent) = match v.state {
        PadState::Empty => ("#181a22", "#8088a0"),
        PadState::Idle => ("#1c212e", "#7aa2f7"),
        PadState::Playing => ("#125234", "#78ecaa"),
        PadState::Recording => ("#841c22", "#ffa0a0"),
        PadState::ClearArmed => ("#7c5414", "#ffd070"),
    };
    let mut body = String::new();

    if !matches!(v.state, PadState::Recording | PadState::ClearArmed) {
        let badge_w = v.badge.chars().count() as f64 * 8.0 + 16.0;
        body += &format!(
            r#"<rect x="10" y="10" width="{badge_w}" height="22" rx="11" fill="{accent}" fill-opacity=".15"/>"#
        );
        body += &text(18.0, 26.0, 12.0, accent, 1.0, "start", &v.badge);
        if v.count > 0 {
            body += &text(
                134.0,
                27.0,
                15.0,
                WHITE,
                0.82,
                "end",
                &format!("×{}", v.count),
            );
        }
    }

    match v.state {
        PadState::Empty => {
            body += &format!(
                r#"
                    <circle cx="72" cy="64" r="24" fill="none" stroke="{accent}" stroke-opacity=".5" stroke-width="4" />
                    <circle cx="72" cy="64" r="11" fill="{RED}" />
                "#
            );
            body += &name_block(&v.name, 108.0, 20.0);
            body += &text(72.0, 134.0, 10.0, accent, 0.75, "middle", "HOLD TO RECORD");
        }
        PadState::Idle | PadState::Playing => {
            body += &name_block(&v.name, 62.0, 26.0);
            let peaks = v.wave.clone().unwrap_or_else(|| vec![0.0; 40]);
            body += &wave(&peaks, v.trim, v.progress, accent);
        }
        PadState::Recording => {
            body += &text(12.0, 28.0, 14.0, WHITE, 0.86, "start", &trunc(&v.name, 14));
            let dot = if v.blink { WHITE } else { "#ff7878" };
            body += &format!(r#"<circle cx="34" cy="64" r="11" fill="{dot}"/>"#);
            body += &text(54.0, 74.0, 28.0, WHITE, 1.0, "start", "REC");
            body += &text(72.0, 124.0, 40.0, WHITE, 1.0, "middle", &mmss(v.secs));
        }
        PadState::ClearArmed => {
            let o = if v.blink { 1.0 } else { 0.6 };
            body += &format!(
                r#"<path d="M54 44L90 80M90 44L54 80" stroke="{accent}" stroke-opacity="{o}" stroke-width="9" stroke-linecap="round"/>"#
            );
            body += &text(72.0, 108.0, 22.0, WHITE, 1.0, "middle", "CLEAR?");
            body += &text(
                72.0,
                130.0,
                12.0,
                accent,
                0.86,
                "middle",
                &trunc(&v.name, 16),
            );
        }
    }

    frame(bg, accent, &body)
}

pub fn icon(v: &IconView) -> String {
    let (bg, accent) = match v.kind {
        IconKind::Idle => ("#1c212e", "#7aa2f7"),
        IconKind::Active => ("#582428", "#ffa0a0"),
        IconKind::Armed => ("#7c5414", "#ffd070"),
        IconKind::Recording => ("#841c22", "#ffa0a0"),
    };
    let mut body = match v.glyph {
        Glyph::Stop => {
            format!(r#"<rect x="46" y="28" width="52" height="52" rx="10" fill="{accent}"/>"#)
        }
        Glyph::Clear => {
            let o = if v.kind == IconKind::Armed && !v.blink {
                0.6
            } else {
                1.0
            };
            format!(
                r#"<path d="M48 32L96 80M96 32L48 80" stroke="{accent}" stroke-opacity="{o}" stroke-width="10" stroke-linecap="round"/>"#
            )
        }
        Glyph::Record => {
            let dot = if v.kind == IconKind::Recording && v.blink {
                WHITE
            } else {
                RED
            };
            format!(
                r#"
                    <circle cx="72" cy="56" r="28" fill="none" stroke="{accent}" stroke-opacity=".6" stroke-width="4"/>
                    <circle cx="72" cy="56" r="15" fill="{dot}"/>
                "#
            )
        }
    };
    body += &text(72.0, 112.0, 22.0, WHITE, 1.0, "middle", &v.label);
    if !v.sub.is_empty() {
        body += &text(
            72.0,
            132.0,
            12.0,
            accent,
            0.86,
            "middle",
            &trunc(&v.sub, 18),
        );
    }
    frame(bg, accent, &body)
}
