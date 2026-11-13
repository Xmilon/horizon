//! The night sky behind the launcher ("Night" in `docs/design-system.md`): banded sky, stars that
//! twinkle in steps, three layers of clouds drifting a texel at a time, a sun cut in slices and two
//! skylines, all drawn by hand below as small sprites.
//!
//! It costs almost nothing at rest: the sky's bands and the stars are plain quads, the sprites are
//! baked once per appearance and scale into tiles whose size doesn't depend on the window, and a
//! frame is asked for only when something on screen changes (a cloud moves one texel, a group of
//! stars steps to its next brightness): a few frames a second. The sky stands still under reduced
//! motion, and stops 10 s after the window loses focus ([`motion`]).
use crate::art::bake;
use gpui::{div, img, prelude::*, px, rgb, Div, RenderImage};
use horizon_theme::night::{Night, SkyArt};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

/// Logical pixels per texel: the sky's bands, the stars and the clouds; the sun and skylines.
const TEXEL: f32 = 4.;
const LAND: f32 = 6.;
/// Each stop of the sky's share of the window's height, top to bottom.
const SHARES: [f32; 6] = [0.2, 0.18, 0.17, 0.15, 0.15, 0.15];
/// How long the sky keeps moving after the window loses focus.
const UNFOCUSED: Duration = Duration::from_secs(10);

/// Clouds: each layer's sprites, placed `(x, y)` in texels on a strip `period` texels wide that
/// repeats across the window, the strip's top as a share of the window's height, and its speed in
/// texels a second. `#` is the cloud, `=` its lit band, `-` its shaded underside.
struct CloudLayer {
    period: usize,
    top: f32,
    speed: f32,
    clouds: &'static [(usize, usize, &'static [&'static str])],
}

const PUFF: &[&str] = &[
    "..............############..................",
    "......##########################............",
    "..##########################################",
    "============================================",
    ".....----------------------------------.....",
];
const LONG: &[&str] = &[
    "......................##############............................",
    "..............##################################................",
    "....############################################################",
    "==============================================================..",
    "..........------------------------------------------............",
    "..............................#################.................",
    "..............................=================.................",
];
const WISP: &[&str] = &[
    "......##################......",
    "==============================",
];
const BANK: &[&str] = &[
    "............##########........................................",
    "......##########################........##############........",
    "..##################################..########################",
    "====================================..========================",
    "......-------------------------.........---------------.......",
];

const CLOUDS: [CloudLayer; 3] = [
    CloudLayer {
        period: 300,
        top: 0.36,
        speed: 0.5,
        clouds: &[(20, 0, WISP), (140, 3, PUFF), (230, 1, WISP)],
    },
    CloudLayer {
        period: 380,
        top: 0.52,
        speed: 1.,
        clouds: &[(30, 1, BANK), (200, 0, PUFF), (300, 4, WISP)],
    },
    CloudLayer {
        period: 460,
        top: 0.64,
        speed: 2.,
        clouds: &[(60, 0, LONG), (290, 2, PUFF)],
    },
];
const CLOUD_ROWS: usize = 10;

/// A skyline: runs of `(width, height)` in texels, repeating every `period` (their widths' sum),
/// with sprites stood on top of the run at `x`. `#` is solid.
struct Skyline {
    runs: &'static [(usize, usize)],
    props: &'static [(usize, &'static [&'static str])],
    rows: usize,
}

const TREE: &[&str] = &[".###.", "#####", "#####", ".###.", "..#..", "..#.."];
const PINE: &[&str] = &["..#..", ".###.", "..#..", ".###.", "#####", "..#.."];
const POST: &[&str] = &[".#.", "###", ".#.", ".#.", ".#."];
const TOWER: &[&str] = &["#.#", "###", "###", "###", "###", "###", "###"];

const FAR: Skyline = Skyline {
    runs: &[
        (18, 8),
        (10, 10),
        (6, 13),
        (14, 11),
        (22, 9),
        (8, 12),
        (12, 14),
        (16, 11),
        (20, 8),
        (10, 10),
        (14, 12),
        (24, 9),
        (12, 11),
        (18, 8),
    ],
    props: &[(32, TOWER), (118, PINE), (176, TOWER)],
    rows: 20,
};

const NEAR: Skyline = Skyline {
    runs: &[
        (12, 7),
        (6, 9),
        (10, 8),
        (4, 11),
        (14, 10),
        (8, 8),
        (6, 6),
        (16, 7),
        (5, 10),
        (9, 12),
        (7, 9),
        (12, 7),
        (10, 8),
        (6, 11),
        (11, 9),
        (9, 7),
        (15, 7),
    ],
    props: &[
        (5, TREE),
        (36, POST),
        (60, PINE),
        (95, TREE),
        (124, POST),
        (139, TREE),
    ],
    rows: 18,
};

/// The sun: its radius in texels, how far it stands above the near skyline's foot, and where
/// across the window (a share of its width).
const SUN_RADIUS: usize = 15;
const SUN_RISE: usize = 8;
const SUN_X: f32 = 0.72;

/// How much white the light look lays over the dawn's colours.
const MORNING: f32 = 0.55;

/// The stars' brightness steps and each group's period: a group steps through the levels in
/// turn, so a star brightens and dims without a frame for every shade in between.
const STAR_LEVELS: [f32; 4] = [0.95, 0.6, 0.3, 0.6];
/// The clouds' speeds and these periods keep every change on a half-second beat, so the changes of
/// different layers share frames: two a second at most.
const STAR_PERIODS: [f32; 4] = [4., 6., 8., 10.];

/// How the sky moves: not at all under reduced motion (the player's choice in Settings, else the
/// system's: Windows' "Show animations", macOS's "Reduce motion") or with
/// `HORIZON_LAUNCHER_MOTION=off`; while the window is in front (and 10 s after); or always, with
/// `HORIZON_LAUNCHER_MOTION=always`, for recordings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Motion {
    Still,
    InFront,
    Always,
}

/// The player's choice (`Settings::reduce_motion`): 0 follows the system, 1 still, 2 moves.
static PREFERENCE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Takes the player's choice: `None` follows the system, `Some(true)` holds everything still.
pub(crate) fn set_preference(reduce: Option<bool>) {
    let v = match reduce {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    };
    PREFERENCE.store(v, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn motion() -> Motion {
    static FORCED: LazyLock<Option<Motion>> =
        LazyLock::new(
            || match std::env::var("HORIZON_LAUNCHER_MOTION").as_deref() {
                Ok("off" | "reduced") => Some(Motion::Still),
                Ok("always") => Some(Motion::Always),
                Ok("full") => Some(Motion::InFront),
                _ => None,
            },
        );
    if let Some(forced) = *FORCED {
        return forced;
    }
    let still = match PREFERENCE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => system_reduces_motion(),
    };
    if still {
        Motion::Still
    } else {
        Motion::InFront
    }
}

/// The system's reduced motion, asked again at most every 2 s, so turning it on or off applies
/// without restarting the launcher.
fn system_reduces_motion() -> bool {
    static ASKED: std::sync::Mutex<Option<(Instant, bool)>> = std::sync::Mutex::new(None);
    let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    match *asked {
        Some((at, reduce)) if now.duration_since(at) < Duration::from_secs(2) => reduce,
        _ => {
            let reduce = ask_system();
            *asked = Some((now, reduce));
            reduce
        }
    }
}

#[cfg(target_os = "windows")]
fn ask_system() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
    };
    let mut animate: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL through the pointer it is given.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animate as *mut i32).cast(),
            0,
        )
    };
    ok != 0 && animate == 0
}

#[cfg(target_os = "macos")]
fn ask_system() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// GNOME's "Animations" switch (`org.gnome.desktop.interface enable-animations`), read by
/// `gsettings` on a worker thread: the answer lands for the next ask, and a desktop without the
/// key or the tool answers "moves".
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn ask_system() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static REDUCE: AtomicBool = AtomicBool::new(false);
    static ASKING: AtomicBool = AtomicBool::new(false);
    if !ASKING.swap(true, Ordering::AcqRel) {
        std::thread::spawn(|| {
            let off = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "enable-animations"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "false");
            REDUCE.store(off, Ordering::Relaxed);
            ASKING.store(false, Ordering::Release);
        });
    }
    REDUCE.load(Ordering::Relaxed)
}

/// The animation's own time: it runs only while the sky moves, so a window that comes back
/// picks up where it stopped instead of jumping.
#[derive(Default)]
pub(crate) struct Clock {
    banked: Duration,
    running: Option<Instant>,
}

impl Clock {
    pub(crate) fn at(&self, now: Instant) -> Duration {
        self.banked + self.running.map_or(Duration::ZERO, |since| now - since)
    }

    pub(crate) fn set(&mut self, run: bool, now: Instant) {
        match (run, self.running) {
            (true, None) => self.running = Some(now),
            (false, Some(since)) => {
                self.banked += now - since;
                self.running = None;
            }
            _ => {}
        }
    }
}

struct Layers {
    clouds: [Arc<RenderImage>; 3],
    far: Arc<RenderImage>,
    near: Arc<RenderImage>,
    sun: Arc<RenderImage>,
}

#[derive(Default)]
pub struct Sky {
    baked: Option<((SkyArt, u32), Layers)>,
    /// Images replaced by a new appearance or scale, still in the window's sprite atlas.
    replaced: Vec<Arc<RenderImage>>,
    clock: Clock,
    unfocused: Option<Instant>,
}

impl Sky {
    /// The sky for a window of `w` by `h` logical pixels, and how long until it next changes
    /// (`None`: it stands still until something else redraws the window).
    pub fn render(
        &mut self,
        n: &Night,
        (w, h): (f32, f32),
        scale: f32,
        focused: bool,
        now: Instant,
    ) -> (Div, Option<Duration>) {
        if focused {
            self.unfocused = None;
        } else {
            self.unfocused.get_or_insert(now);
        }
        let run = match motion() {
            Motion::Still => false,
            Motion::InFront => self.unfocused.is_none_or(|since| now - since < UNFOCUSED),
            Motion::Always => true,
        };
        self.clock.set(run, now);
        let t = self.clock.at(now).as_secs_f32();
        let layers = self.layers(n.art, scale);
        let snap = |v: f32| (v * scale).round() / scale;
        let art = &n.art;

        let mut sky = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden();
        for (top, height, c) in bands(n, h) {
            sky = sky.child(
                div()
                    .absolute()
                    .left_0()
                    .w_full()
                    .top(px(top))
                    .h(px(height))
                    .bg(rgb(c)),
            );
        }
        for (g, period) in STAR_PERIODS.iter().enumerate() {
            let level = STAR_LEVELS[step(t, *period, g)];
            let mut group = div().absolute().top_0().left_0().size_full().opacity(level);
            for star in stars().iter().filter(|s| s.group == g) {
                let (x, y) = (star.x * TEXEL, star.y * h * 0.5);
                if x >= w {
                    continue;
                }
                let size = if star.big { TEXEL / 2. } else { TEXEL / 4. };
                group = group.child(
                    div()
                        .absolute()
                        .left(px(snap(x)))
                        .top(px(snap(y)))
                        .size(px(size.max(1. / scale)))
                        .bg(rgb(art.star)),
                );
            }
            sky = sky.child(group);
        }
        for (layer, image) in CLOUDS.iter().zip(&layers.clouds) {
            let strip = layer.period as f32 * TEXEL;
            let shift = (t * layer.speed).floor() * TEXEL % strip;
            let top = snap(h * layer.top);
            let mut x = -shift;
            while x < w {
                sky = sky.child(
                    img(image.clone())
                        .absolute()
                        .left(px(x))
                        .top(px(top))
                        .w(px(strip))
                        .h(px(CLOUD_ROWS as f32 * TEXEL)),
                );
                x += strip;
            }
        }
        let foot = h;
        let sun_w = (SUN_RADIUS * 2) as f32 * LAND;
        sky = sky.child(
            img(layers.sun.clone())
                .absolute()
                .left(px(snap(w * SUN_X - sun_w / 2.)))
                .top(px(foot - (SUN_RISE + SUN_RADIUS) as f32 * LAND))
                .w(px(sun_w))
                .h(px(SUN_RADIUS as f32 * LAND)),
        );
        for (line, image, lift) in [(&FAR, &layers.far, 2.), (&NEAR, &layers.near, 0.)] {
            let period = line.runs.iter().map(|r| r.0).sum::<usize>() as f32 * LAND;
            let top = foot - (line.rows as f32 + lift) * LAND;
            // The far line starts half a period in, so the two never repeat in step.
            let mut x = if lift > 0. { -period / 2. } else { 0. };
            while x < w {
                sky = sky.child(
                    img(image.clone())
                        .absolute()
                        .left(px(x))
                        .top(px(top))
                        .w(px(period))
                        .h(px(line.rows as f32 * LAND)),
                );
                x += period;
            }
            if lift > 0. {
                sky = sky.child(
                    div()
                        .absolute()
                        .left_0()
                        .w_full()
                        .top(px(foot - lift * LAND))
                        .h(px(lift * LAND))
                        .bg(rgb(art.far)),
                );
            }
        }

        let next = self.clock.running.is_some().then(|| {
            let clouds = CLOUDS.iter().map(|l| until_next(t * l.speed) / l.speed);
            let stars = STAR_PERIODS.iter().enumerate().map(|(g, p)| {
                let quarter = p / STAR_LEVELS.len() as f32;
                until_next(t / quarter + g as f32) * quarter
            });
            let mut soonest = clouds.chain(stars).fold(f32::MAX, f32::min);
            if let (Some(since), Motion::InFront) = (self.unfocused, motion()) {
                soonest = soonest.min((UNFOCUSED - (now - since)).as_secs_f32());
            }
            // A little after the change, so the frame's clock is surely past it.
            Duration::from_secs_f32(soonest.max(0.) + 0.005)
        });
        (sky, next)
    }

    fn layers(&mut self, art: SkyArt, scale: f32) -> &Layers {
        let key = (art, (scale * 100.).round() as u32);
        if !matches!(&self.baked, Some((k, _)) if *k == key) {
            let art = &art;
            let layers = Layers {
                clouds: std::array::from_fn(|i| {
                    let l = &CLOUDS[i];
                    bake(&clouds(l, art, i), l.period, CLOUD_ROWS, TEXEL, scale)
                }),
                far: skyline(&FAR, art.far, scale),
                near: skyline(&NEAR, art.near, scale),
                sun: bake(&sun(art), SUN_RADIUS * 2, SUN_RADIUS, LAND, scale),
            };
            if let Some((_, old)) = self.baked.replace((key, layers)) {
                self.replaced
                    .extend(old.clouds.into_iter().chain([old.far, old.near, old.sun]));
            }
        }
        &self.baked.as_ref().expect("baked above").1
    }

    /// Frees the images replaced since the last frame (GPUI keeps them in the sprite atlas until
    /// they are dropped there).
    pub fn free_replaced(&mut self, window: &mut gpui::Window) {
        for image in self.replaced.drain(..) {
            let _ = window.drop_image(image);
        }
    }
}

/// Which of a star group's levels shows at `t` seconds.
fn step(t: f32, period: f32, group: usize) -> usize {
    // Groups start a quarter period apart, which keeps their steps on the beat.
    let phase = group as f32 * 0.25;
    ((t / period + phase).fract() * STAR_LEVELS.len() as f32) as usize % STAR_LEVELS.len()
}

/// How far `v` is from the next whole number.
fn until_next(v: f32) -> f32 {
    v.floor() + 1. - v
}

/// The sky's bands, `(top, height, colour)` in logical pixels on whole texels: each stop of the
/// sky in four steps toward the next, the stops sharing the height as [`SHARES`] says.
fn bands(n: &Night, h: f32) -> Vec<(f32, f32, u32)> {
    const STEPS: usize = 4;
    // The light look's sky is the dawn's washed with white, so the page reads white with a tint
    // of the dawn rather than lilac.
    let wash = |c: u32| {
        if n.dark {
            c
        } else {
            horizon_theme::night::over(0xffffff, MORNING, c)
        }
    };
    let rows = (h / TEXEL).ceil();
    let mut out = Vec::with_capacity(SHARES.len() * STEPS);
    let mut at = 0.;
    for (i, share) in SHARES.iter().enumerate() {
        let span = (share * rows).round().max(STEPS as f32);
        let Some(&next) = n.sky.get(i + 1) else {
            out.push((
                at * TEXEL,
                (h / TEXEL - at).max(span) * TEXEL,
                wash(n.sky[i]),
            ));
            break;
        };
        let mut from = at;
        for k in 0..STEPS {
            let to = (at + span * (k + 1) as f32 / STEPS as f32).round();
            let c = horizon_theme::night::over(next, k as f32 / STEPS as f32, n.sky[i]);
            out.push((from * TEXEL, (to - from) * TEXEL, wash(c)));
            from = to;
        }
        at += span;
    }
    out
}

struct Star {
    /// Column in texels, row as a share of the upper half of the window.
    x: f32,
    y: f32,
    group: usize,
    big: bool,
}

/// The stars, the same on every start: a fixed field wide enough for any window, thinned toward
/// the horizon.
fn stars() -> &'static [Star] {
    static STARS: LazyLock<Vec<Star>> = LazyLock::new(|| {
        let mut rng = 0x2545_f491u32;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            (rng >> 8) as f32 / (1 << 24) as f32
        };
        (0..240)
            .filter_map(|i| {
                let (x, y) = ((next() * 960.).floor(), next());
                let keep = next() > y * y;
                let big = next() < 0.08;
                keep.then_some(Star {
                    x,
                    y,
                    group: i % STAR_PERIODS.len(),
                    big,
                })
            })
            .collect()
    });
    &STARS
}

/// `rows` of `#`-art at `(x, y)` in `out` (`w` texels wide), each mark coloured by `paint`.
fn stamp(
    out: &mut [[u8; 4]],
    w: usize,
    (x, y): (usize, usize),
    rows: &[&str],
    paint: impl Fn(u8) -> Option<u32>,
) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, mark) in row.bytes().enumerate() {
            let (px, py) = ((x + dx) % w, y + dy);
            if let (Some(c), Some(cell)) = (paint(mark), out.get_mut(py * w + px)) {
                *cell = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
            }
        }
    }
}

fn clouds(layer: &CloudLayer, art: &SkyArt, depth: usize) -> Vec<[u8; 4]> {
    let mut out = vec![[0; 4]; layer.period * CLOUD_ROWS];
    for &(x, y, sprite) in layer.clouds {
        stamp(&mut out, layer.period, (x, y), sprite, |m| match m {
            b'#' => Some(art.cloud[depth]),
            b'=' => Some(art.lit[depth]),
            b'-' => Some(art.shade),
            _ => None,
        });
    }
    out
}

fn skyline(line: &Skyline, color: u32, scale: f32) -> Arc<RenderImage> {
    let w: usize = line.runs.iter().map(|r| r.0).sum();
    let mut out = vec![[0; 4]; w * line.rows];
    let mut x = 0;
    let mut tops = vec![0; w];
    for &(run, height) in line.runs {
        for top in &mut tops[x..x + run] {
            *top = line.rows - height;
        }
        x += run;
    }
    for (x, &top) in tops.iter().enumerate() {
        for y in top..line.rows {
            out[y * w + x] = [(color >> 16) as u8, (color >> 8) as u8, color as u8, 255];
        }
    }
    for &(x, sprite) in line.props {
        let width = sprite[0].len();
        let ground = tops[x..x + width]
            .iter()
            .copied()
            .min()
            .unwrap_or(line.rows);
        let y = ground.saturating_sub(sprite.len());
        stamp(&mut out, w, (x, y), sprite, |m| {
            (m == b'#').then_some(color)
        });
    }
    bake(&out, w, line.rows, LAND, scale)
}

/// The upper half of a sun cut by the horizon, in three bands of colour, its lower half sliced by
/// rows of sky like Horizon's mark.
fn sun(art: &SkyArt) -> Vec<[u8; 4]> {
    let (r, w) = (SUN_RADIUS as f32, SUN_RADIUS * 2);
    let mut out = vec![[0; 4]; w * SUN_RADIUS];
    for y in 0..SUN_RADIUS {
        let dy = r - y as f32 - 0.5;
        let half = (r * r - dy * dy).sqrt().round() as usize;
        let depth = y as f32 / r;
        let sliced = depth > 0.5 && (SUN_RADIUS - y).is_multiple_of(3);
        if sliced {
            continue;
        }
        let c = art.sun[if depth < 0.4 {
            0
        } else if depth < 0.7 {
            1
        } else {
            2
        }];
        for x in SUN_RADIUS - half..SUN_RADIUS + half {
            out[y * w + x] = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_theme::night::{DAWN, NIGHT, UNCARDED_STOPS};

    /// The sky's upper bands, behind text without a plate, hold the night tokens' text colours.
    fn uncarded_bands(n: &Night, h: f32) -> Vec<u32> {
        let limit: f32 = SHARES[..UNCARDED_STOPS].iter().sum::<f32>() * h;
        bands(n, h)
            .into_iter()
            .filter(|(top, _, _)| *top < limit)
            .map(|(_, _, c)| c)
            .collect()
    }

    #[test]
    fn the_bands_cover_the_window_on_whole_texels() {
        for h in [600., 777., 1080.] {
            let bands = bands(&NIGHT, h);
            let mut at = 0.;
            for (top, height, _) in &bands {
                assert_eq!(*top, at);
                assert_eq!(top % TEXEL, 0.);
                at += height;
            }
            assert!(at >= h, "{at} < {h}");
        }
    }

    #[test]
    fn text_without_a_plate_reads_on_the_upper_bands() {
        for n in [NIGHT, DAWN] {
            for c in uncarded_bands(&n, 800.) {
                for text in [n.ink, n.ink_soft, n.muted] {
                    let ratio = horizon_theme::contrast(text, c);
                    assert!(ratio >= 4.5, "#{text:06x} on #{c:06x}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn the_art_fits_its_tiles() {
        for layer in &CLOUDS {
            for (x, y, sprite) in layer.clouds {
                assert!(y + sprite.len() <= CLOUD_ROWS);
                assert!(
                    x + sprite[0].len() <= layer.period,
                    "a cloud crosses its strip's end"
                );
                assert!(sprite.iter().all(|r| r.len() == sprite[0].len()));
            }
        }
        for line in [&FAR, &NEAR] {
            let w: usize = line.runs.iter().map(|r| r.0).sum();
            assert!(line.runs.iter().all(|r| r.1 < line.rows));
            // The tile's ends meet: the line repeats without a step.
            assert_eq!(line.runs[0].1, line.runs[line.runs.len() - 1].1);
            for (x, sprite) in line.props {
                assert!(x + sprite[0].len() <= w);
            }
        }
    }

    #[test]
    fn every_change_falls_on_the_beat() {
        const BEAT: f32 = 0.5;
        let on_beat = |v: f32| ((v / BEAT).round() * BEAT - v).abs() < 1e-4;
        for layer in &CLOUDS {
            assert!(
                on_beat(1. / layer.speed),
                "a cloud layer steps every {} s",
                1. / layer.speed
            );
        }
        for (g, p) in STAR_PERIODS.iter().enumerate() {
            let quarter = p / STAR_LEVELS.len() as f32;
            assert!(
                on_beat(quarter) && on_beat(g as f32 * 0.25 * p),
                "star group {g}"
            );
        }
    }

    #[test]
    fn the_clock_stops_and_resumes_without_a_jump() {
        let start = Instant::now();
        let mut clock = Clock::default();
        clock.set(true, start);
        let s = Duration::from_secs(1);
        clock.set(false, start + 2 * s);
        assert_eq!(clock.at(start + 10 * s), 2 * s);
        clock.set(true, start + 10 * s);
        assert_eq!(clock.at(start + 11 * s), 3 * s);
    }

    #[test]
    fn frames_come_only_when_something_moves() {
        assert!((until_next(0.) - 1.).abs() < 1e-6);
        let mut sky = Sky::default();
        let now = Instant::now();
        let (_, next) = sky.render(&NIGHT, (1280., 800.), 2., true, now);
        if motion() != Motion::Still {
            let next = next.expect("a moving sky asks for its next frame");
            assert!(next <= Duration::from_secs(1), "{next:?}");
        }
        // Unfocused for longer than UNFOCUSED: no more frames.
        if motion() != Motion::InFront {
            return;
        }
        sky.render(&NIGHT, (1280., 800.), 2., false, now);
        let (_, next) = sky.render(&NIGHT, (1280., 800.), 2., false, now + UNFOCUSED);
        assert_eq!(next, None);
    }
}
