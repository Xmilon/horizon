//! The player's own look, on top of Dark, Light or System: an accent colour and a picture behind
//! Home.
//!
//! An accent is a hue, and the player may pick any of them; seven hues have names, as shortcuts.
//! Horizon's violet is the default. Another hue turns the violet tokens (`horizon_theme::Blocks`'
//! accent and the main action's face, bevel and lip) to it, keeping their saturation and
//! lightness, then darkens or lightens each just enough to read as well as the violet does: white
//! on the face at 4.5:1, the accent at 3:1 on every background.
use horizon_theme::night::{Mood, Night, SkyArt};
use horizon_theme::{contrast, Blocks};
use std::path::{Path, PathBuf};

/// An accent: a hue in degrees, or none for Horizon's violet, whose tokens are the palette's own
/// and stay untouched; and for a colour typed as hex, that colour (the glass look wears it as
/// is, the light look takes its hue).
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Accent {
    hue: Option<f32>,
    rgb: Option<u32>,
}

/// The accent a launcher theme brings: a hue in degrees (First Light) or one exact colour (Nord's
/// frost).
#[derive(Clone, Copy, Debug)]
pub enum ThemeAccent {
    Hue(f32),
    Colour(u32),
}

impl ThemeAccent {
    pub fn accent(self) -> Accent {
        match self {
            Self::Hue(hue) => Accent::custom(hue),
            Self::Colour(rgb) => Accent::from_rgb(rgb),
        }
    }
}

/// How a theme with its own palette draws the launcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// The same glass pages as Horizon's own, over the theme's skies: the dark one is the pixel
    /// sunset in the accent's hue, the light one the theme's light sky.
    Glass,
    /// The glass pages with solid plates and a thin edge over the theme's own dark sky (true black
    /// for OLED screens) instead of the sunset; the light appearance as [`Look::Glass`].
    Oled,
    /// The glass pages over the theme's own painted sky (`sky.rs`, with its drifting
    /// [`Mood`](horizon_theme::night::Mood)) instead of the sunset, on solid plates; the light
    /// appearance as [`Look::Glass`].
    Painted,
    /// The square, solid Blocks look (Nord, kept as it was).
    Solid,
}

/// A launcher theme: free for everyone in Settings › Appearance, never sold or claimed. Each
/// has a sky (six stops, top to bottom, behind the light appearance) and the accent it brings.
/// A theme that brings a whole palette carries its dark and light [`Night`] and paints with it
/// as its [`Look`] says; the other themes only change the sky.
pub struct Theme {
    /// Its name in `Settings::theme`.
    pub id: &'static str,
    /// Its name in Settings, in English (through `tr`).
    pub name: &'static str,
    pub sky: [u32; 6],
    pub accent: ThemeAccent,
    /// The theme's own palette (dark, light), for a theme that is more than a sky.
    pub palette: Option<(&'static Night, &'static Night)>,
    /// How the palette is drawn; unused without one.
    pub look: Look,
}

/// Nord's polar night and snow storm, with the frost accent.
pub const NORD_DARK: Night = Night {
    sky: [0x2e3440, 0x2e3440, 0x313845, 0x353d4b, 0x3b4252, 0x434c5e],
    page: 0x2e3440,
    side: 0x292e39,
    card: 0x3b4252,
    card_alpha: 0.88,
    card_hi: 0x434c5e,
    card_hi_alpha: 0.92,
    stroke: 0xd8dee9,
    stroke_alpha: 0.10,
    rule_alpha: 0.10,
    ink: 0xeceff4,
    ink_soft: 0xd8dee9,
    muted: 0xb6bfca,
    accent: 0x88c0d0,
    accent_hi: 0x9fd3d3,
    on_accent: 0x2e3440,
    success: 0xafc99a,
    warn: 0xebcb8b,
    danger: 0xeeb3b8,
    dark: true,
    art: SkyArt {
        sun: [0x8fbcbb, 0x88c0d0, 0x5e81ac],
        far: 0x3b4252,
        near: 0x272c36,
        cloud: [0x3b4252, 0x434c5e, 0x4c566a],
        lit: [0x4c566a, 0x5e6a7d, 0x81a1c1],
        shade: 0x272c36,
        star: 0xeceff4,
        mood: Mood::Plain,
    },
};

pub const NORD_LIGHT: Night = Night {
    sky: [0xeceff4, 0xeceff4, 0xe5e9f0, 0xe5e9f0, 0xd8dee9, 0xc8d2e0],
    page: 0xeceff4,
    side: 0xe5e9f0,
    card: 0xf8f9fb,
    card_alpha: 0.90,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x4c566a,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x2e3440,
    ink_soft: 0x434c5e,
    muted: 0x536070,
    accent: 0x4c6a92,
    accent_hi: 0x4c6a92,
    on_accent: 0xffffff,
    success: 0x3f6b2e,
    warn: 0x7a5d00,
    danger: 0xa8323f,
    dark: false,
    art: SkyArt {
        sun: [0xffffff, 0xeceff4, 0xd8dee9],
        far: 0xd8dee9,
        near: 0xc8d2e0,
        cloud: [0xf8f9fb, 0xeceff4, 0xe5e9f0],
        lit: [0xffffff; 3],
        shade: 0xd8dee9,
        star: 0xffffff,
        mood: Mood::Plain,
    },
};

/// First Light: the sky just before sunrise, with a harbour skyline.
pub const FIRST_LIGHT: Theme = Theme {
    id: "first_light",
    name: "First Light",
    sky: [0x0b1030, 0x1d2a63, 0x3d4c94, 0x7f78c0, 0xe2a0b0, 0xffd6a0],
    accent: ThemeAccent::Hue(246.),
    palette: None,
    look: Look::Glass,
};

/// Nord: the Nord palette's polar night, snow storm and frost.
pub const NORD: Theme = Theme {
    id: "nord",
    name: "Nord",
    sky: NORD_DARK.sky,
    accent: ThemeAccent::Colour(0x88c0d0),
    palette: Some((&NORD_DARK, &NORD_LIGHT)),
    look: Look::Solid,
};

/// Sunrise's night: the website's Dawn palette (plum canvas, rose and peach, warm white text).
pub const SUNRISE_DARK: Night = Night {
    sky: [0x170c19, 0x1f0f20, 0x2c1229, 0x5a1d3a, 0x9c3a5a, 0xf08a6a],
    page: 0x170c19,
    side: 0x130a15,
    card: 0x2a1428,
    card_alpha: 0.88,
    card_hi: 0x3a1c36,
    card_hi_alpha: 0.92,
    stroke: 0xffb3cf,
    stroke_alpha: 0.12,
    rule_alpha: 0.10,
    ink: 0xfff1f6,
    ink_soft: 0xf1d3df,
    muted: 0xcfa9bb,
    accent: 0xc2366f,
    accent_hi: 0xff9ec0,
    on_accent: 0xffffff,
    success: 0x6fdba0,
    warn: 0xffc48f,
    danger: 0xff9a9a,
    dark: true,
    art: SkyArt {
        sun: [0xffe0a8, 0xffc48f, 0xff9a6e],
        far: 0x3d1a3a,
        near: 0x1a0c1a,
        cloud: [0x4a1e44, 0x8a3460, 0xd96a86],
        lit: [0x6a2c5a, 0xb4507a, 0xffb08f],
        shade: 0x2a1228,
        star: 0xfff1f6,
        mood: Mood::Plain,
    },
};

/// Sunrise's day: warm off-white and blush, dark plum text, a deeper rose.
pub const SUNRISE_LIGHT: Night = Night {
    sky: [0xf6e9f2, 0xfbe7ec, 0xffe6e2, 0xffe2d6, 0xffdcc6, 0xffe9d2],
    page: 0xfff7f5,
    side: 0xfbeff0,
    card: 0xffffff,
    card_alpha: 0.88,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x5a2348,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x32182e,
    ink_soft: 0x4f2a48,
    muted: 0x6e4a66,
    accent: 0x9d285b,
    accent_hi: 0x8f2252,
    on_accent: 0xffffff,
    success: 0x1f6a3a,
    warn: 0x7a4a00,
    danger: 0xa3262c,
    dark: false,
    art: SkyArt {
        sun: [0xfffaf0, 0xffeacc, 0xffd2b0],
        far: 0xf0d6e2,
        near: 0xdcb8cc,
        cloud: [0xfdf0f4, 0xffeef0, 0xfff3e6],
        lit: [0xffffff; 3],
        shade: 0xecd3df,
        star: 0xffffff,
        mood: Mood::Plain,
    },
};

/// Hypr Neon's night: true black, lavender, blue and pink, thin visible edges (after the
/// player's Hypr OLED NEON Discord theme; its colours only, nothing of its CSS).
pub const HYPR_DARK: Night = Night {
    sky: [0x000000, 0x000000, 0x020205, 0x05050c, 0x0a0a16, 0x120f22],
    page: 0x000000,
    side: 0x000000,
    card: 0x09090e,
    card_alpha: 0.96,
    card_hi: 0x12121b,
    card_hi_alpha: 0.97,
    stroke: 0xcba6f7,
    stroke_alpha: 0.22,
    rule_alpha: 0.12,
    ink: 0xe6e9f8,
    ink_soft: 0xc8cce0,
    muted: 0x9aa0bd,
    accent: 0xcba6f7,
    accent_hi: 0xcba6f7,
    on_accent: 0x100a1f,
    success: 0xa6e3a1,
    warn: 0xf9e2af,
    danger: 0xf38ba8,
    dark: true,
    art: SkyArt {
        sun: [0xf5c2e7, 0xcba6f7, 0x89b4fa],
        far: 0x0d0d18,
        near: 0x050508,
        cloud: [0x0b0b14, 0x10101c, 0x161626],
        lit: [0x1a1a2e, 0x22223a, 0x2c2c4a],
        shade: 0x08080f,
        star: 0xcdd6f4,
        mood: Mood::Plain,
    },
};

/// Hypr Neon's day ("paper"): pale lavender-white, dark ink, deeper lavender. Not OLED.
pub const HYPR_LIGHT: Night = Night {
    sky: [0xf6f3fb, 0xf3effa, 0xefeaf8, 0xebe5f6, 0xe6e0f4, 0xe2dcf2],
    page: 0xf6f3fb,
    side: 0xefeaf8,
    card: 0xffffff,
    card_alpha: 0.94,
    card_hi: 0xffffff,
    card_hi_alpha: 0.97,
    stroke: 0x6940a5,
    stroke_alpha: 0.22,
    rule_alpha: 0.12,
    ink: 0x211b2e,
    ink_soft: 0x3a3150,
    muted: 0x5a5170,
    accent: 0x6940a5,
    accent_hi: 0x5e3896,
    on_accent: 0xffffff,
    success: 0x2c6e2a,
    warn: 0x7a5200,
    danger: 0xb0244a,
    dark: false,
    art: SkyArt {
        sun: [0xffffff, 0xf5ecff, 0xe6d8fb],
        far: 0xe2d8f2,
        near: 0xcfc2e8,
        cloud: [0xfbf9ff, 0xf6f1ff, 0xf1eaff],
        lit: [0xffffff; 3],
        shade: 0xe0d6f0,
        star: 0xffffff,
        mood: Mood::Plain,
    },
};

/// Sunrise: the Sunrise website's warm dawn, on Horizon's glass pages.
pub const SUNRISE: Theme = Theme {
    id: "sunrise",
    name: "Sunrise",
    sky: SUNRISE_DARK.sky,
    accent: ThemeAccent::Colour(0xc2366f),
    palette: Some((&SUNRISE_DARK, &SUNRISE_LIGHT)),
    look: Look::Glass,
};

/// Hypr Neon: OLED black with neon lavender, on Horizon's pages with solid plates.
pub const HYPR_NEON: Theme = Theme {
    id: "hypr_neon",
    name: "Hypr Neon",
    sky: HYPR_DARK.sky,
    accent: ThemeAccent::Colour(0xcba6f7),
    palette: Some((&HYPR_DARK, &HYPR_LIGHT)),
    look: Look::Oled,
};

/// Deep Ocean's night: teal water under light shafts and rising bubbles.
pub const OCEAN_DARK: Night = Night {
    sky: [0x031a24, 0x052a38, 0x083c4e, 0x0b5468, 0x10708a, 0x2a9bb0],
    page: 0x041c26,
    side: 0x03141c,
    card: 0x07242f,
    card_alpha: 0.88,
    card_hi: 0x0d3342,
    card_hi_alpha: 0.92,
    stroke: 0x9fe8f2,
    stroke_alpha: 0.12,
    rule_alpha: 0.10,
    ink: 0xeafcff,
    ink_soft: 0xc4e6ec,
    muted: 0x93bcc6,
    accent: 0x2cc4d6,
    accent_hi: 0x7fe3ee,
    on_accent: 0x03212b,
    success: 0x6fe3a8,
    warn: 0xffd37a,
    danger: 0xff9a9a,
    dark: true,
    art: SkyArt {
        sun: [0xd8fbff, 0xa6eef7, 0x62d4e6],
        far: 0x0a4458,
        near: 0x04202b,
        cloud: [0x0b4a5c, 0x11677c, 0x2a8fa3],
        lit: [0x13607a, 0x2288a0, 0x5cc0d0],
        shade: 0x06303f,
        star: 0xdffaff,
        mood: Mood::Water([0x8feeff, 0xbff6ff, 0xffffff]),
    },
};

/// Deep Ocean's day: shallow, sunlit water.
pub const OCEAN_LIGHT: Night = Night {
    sky: [0xe3f6fa, 0xd5f0f6, 0xc6e9f1, 0xb7e1ec, 0xa9d9e7, 0xbfe6ef],
    page: 0xeaf8fb,
    side: 0xdff1f5,
    card: 0xffffff,
    card_alpha: 0.88,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x0b4a5c,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x08303c,
    ink_soft: 0x16495a,
    muted: 0x3f6674,
    accent: 0x0a7388,
    accent_hi: 0x085f72,
    on_accent: 0xffffff,
    success: 0x1b6b3f,
    warn: 0x7a4f00,
    danger: 0xa3262c,
    dark: false,
    art: SkyArt {
        sun: [0xffffff, 0xeafcff, 0xcdf2f8],
        far: 0xc4e6ee,
        near: 0xa3d3e0,
        cloud: [0xf3fcfe, 0xeaf8fb, 0xe0f4f8],
        lit: [0xffffff; 3],
        shade: 0xcde8ef,
        star: 0xffffff,
        mood: Mood::Water([0xffffff, 0x5db8cc, 0xffffff]),
    },
};

/// Deep Ocean: teal water, light shafts and bubbles, on Horizon's glass pages.
pub const DEEP_OCEAN: Theme = Theme {
    id: "deep_ocean",
    name: "Deep Ocean",
    sky: OCEAN_DARK.sky,
    accent: ThemeAccent::Colour(0x2cc4d6),
    palette: Some((&OCEAN_DARK, &OCEAN_LIGHT)),
    look: Look::Painted,
};

/// Autumn Forest's night: embers of amber and rust, leaves falling.
pub const AUTUMN_DARK: Night = Night {
    sky: [0x140b07, 0x24120a, 0x3a1b0d, 0x5e2a10, 0x9a4a14, 0xe08a2e],
    page: 0x160c07,
    side: 0x120907,
    card: 0x2a1509,
    card_alpha: 0.88,
    card_hi: 0x3b1f10,
    card_hi_alpha: 0.92,
    stroke: 0xffc58a,
    stroke_alpha: 0.12,
    rule_alpha: 0.10,
    ink: 0xfff4e6,
    ink_soft: 0xf0d8bd,
    muted: 0xc8a78a,
    accent: 0xe0782a,
    accent_hi: 0xffb26b,
    on_accent: 0x1e0e05,
    success: 0xb5d96b,
    warn: 0xffcf6b,
    danger: 0xff9a8a,
    dark: true,
    art: SkyArt {
        sun: [0xffe3a0, 0xffbf5c, 0xf0892a],
        far: 0x3a1c0e,
        near: 0x160a05,
        cloud: [0x3d1d0f, 0x6e3414, 0xb8621f],
        lit: [0x5a2c14, 0x9a4d1a, 0xe89a4a],
        shade: 0x2a1409,
        star: 0xffe9c8,
        mood: Mood::Leaves([0xd9531e, 0xf0a02a, 0x9c3a14]),
    },
};

/// Autumn Forest's day: warm paper and a deeper rust.
pub const AUTUMN_LIGHT: Night = Night {
    sky: [0xfbf1e1, 0xf9e8d0, 0xf6dcc0, 0xf2d0b0, 0xefc6a0, 0xf6dbbb],
    page: 0xfdf6ea,
    side: 0xf8ecdb,
    card: 0xffffff,
    card_alpha: 0.88,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x5a2c10,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x2e1708,
    ink_soft: 0x4d2b14,
    muted: 0x6c4a33,
    accent: 0xa8420f,
    accent_hi: 0x8f3709,
    on_accent: 0xffffff,
    success: 0x3d6b1c,
    warn: 0x7a4a00,
    danger: 0xa3262c,
    dark: false,
    art: SkyArt {
        sun: [0xfffaf0, 0xffe9c4, 0xffd49b],
        far: 0xf0d5b8,
        near: 0xdcb28a,
        cloud: [0xfff6ea, 0xfdeedd, 0xfbe7d2],
        lit: [0xffffff; 3],
        shade: 0xecd2b5,
        star: 0xffffff,
        mood: Mood::Leaves([0xc4521a, 0xe08a1e, 0x8f3a12]),
    },
};

/// Autumn Forest: amber and rust under falling leaves, on Horizon's glass pages.
pub const AUTUMN_FOREST: Theme = Theme {
    id: "autumn_forest",
    name: "Autumn Forest",
    sky: AUTUMN_DARK.sky,
    accent: ThemeAccent::Colour(0xe0782a),
    palette: Some((&AUTUMN_DARK, &AUTUMN_LIGHT)),
    look: Look::Painted,
};

/// Cherry Blossom's night: a lavender twilight with pink petals drifting.
pub const CHERRY_DARK: Night = Night {
    sky: [0x120d1f, 0x1f1633, 0x32214d, 0x4f3166, 0x7d4a80, 0xc98bb0],
    page: 0x140e22,
    side: 0x0f0a1b,
    card: 0x251a3a,
    card_alpha: 0.88,
    card_hi: 0x32244d,
    card_hi_alpha: 0.92,
    stroke: 0xffd0e4,
    stroke_alpha: 0.12,
    rule_alpha: 0.10,
    ink: 0xfff4fa,
    ink_soft: 0xead8ef,
    muted: 0xb9a6cc,
    accent: 0xf59ac0,
    accent_hi: 0xffc2da,
    on_accent: 0x2a0f1f,
    success: 0x8fe3b0,
    warn: 0xffd59a,
    danger: 0xff9aa8,
    dark: true,
    art: SkyArt {
        sun: [0xfff0f6, 0xffd0e2, 0xf5a0c0],
        far: 0x3a2655,
        near: 0x160f28,
        cloud: [0x3a2a5c, 0x6a4a86, 0xb87ca8],
        lit: [0x4f3a74, 0x8b64a0, 0xe8a8c8],
        shade: 0x2a1d44,
        star: 0xfff0fa,
        mood: Mood::Petals([0xffc2d9, 0xff8fb8]),
    },
};

/// Cherry Blossom's day: blush white with a deep rose.
pub const CHERRY_LIGHT: Night = Night {
    sky: [0xfff2f7, 0xffe9f1, 0xffe0ec, 0xffd8e6, 0xf9d0e0, 0xfde0ea],
    page: 0xfff7fa,
    side: 0xfdecf2,
    card: 0xffffff,
    card_alpha: 0.88,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x6b2a4a,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x3a1426,
    ink_soft: 0x5a2a42,
    muted: 0x7a5468,
    accent: 0xb82f68,
    accent_hi: 0xa02558,
    on_accent: 0xffffff,
    success: 0x1f6a3a,
    warn: 0x7a4a00,
    danger: 0xa3262c,
    dark: false,
    art: SkyArt {
        sun: [0xffffff, 0xfff0f5, 0xffdbe8],
        far: 0xf6d3e0,
        near: 0xe6b1c8,
        cloud: [0xfffafc, 0xfff2f7, 0xffeaf1],
        lit: [0xffffff; 3],
        shade: 0xf2d0de,
        star: 0xffffff,
        mood: Mood::Petals([0xf48fb1, 0xe0608f]),
    },
};

/// Cherry Blossom: a pink twilight with drifting petals, on Horizon's glass pages.
pub const CHERRY_BLOSSOM: Theme = Theme {
    id: "cherry_blossom",
    name: "Cherry Blossom",
    sky: CHERRY_DARK.sky,
    accent: ThemeAccent::Colour(0xf59ac0),
    palette: Some((&CHERRY_DARK, &CHERRY_LIGHT)),
    look: Look::Painted,
};

/// Midnight Aurora's night: deep blue with curtains of green and violet light.
pub const AURORA_DARK: Night = Night {
    sky: [0x02060f, 0x040c1c, 0x071428, 0x0b1e34, 0x10304a, 0x1a4a5c],
    page: 0x030812,
    side: 0x02050d,
    card: 0x0a1426,
    card_alpha: 0.88,
    card_hi: 0x122238,
    card_hi_alpha: 0.92,
    stroke: 0xa8f0d0,
    stroke_alpha: 0.12,
    rule_alpha: 0.10,
    ink: 0xeefff8,
    ink_soft: 0xcce8e0,
    muted: 0x92b8b0,
    accent: 0x4fe0a0,
    accent_hi: 0x9af5cc,
    on_accent: 0x03140e,
    success: 0x7ff0a8,
    warn: 0xffd98a,
    danger: 0xff9aa0,
    dark: true,
    art: SkyArt {
        sun: [0xeafff5, 0xbdf5dd, 0x7fe0b8],
        far: 0x0d2236,
        near: 0x040a14,
        cloud: [0x0a1a2e, 0x10304a, 0x1d4a60],
        lit: [0x14304a, 0x1f5068, 0x3c8a90],
        shade: 0x07121f,
        star: 0xeefaff,
        mood: Mood::Aurora([0x3dffa0, 0x9b6bff, 0x35d6ff]),
    },
};

/// Midnight Aurora's day: cool mist with pale green and violet veils.
pub const AURORA_LIGHT: Night = Night {
    sky: [0xeaf5f2, 0xe3f0f2, 0xdeeaf4, 0xdce4f4, 0xdfe0f4, 0xe8e4f6],
    page: 0xf0f8f6,
    side: 0xe6f1f0,
    card: 0xffffff,
    card_alpha: 0.88,
    card_hi: 0xffffff,
    card_hi_alpha: 0.95,
    stroke: 0x1d3a4a,
    stroke_alpha: 0.14,
    rule_alpha: 0.10,
    ink: 0x0e2630,
    ink_soft: 0x24434f,
    muted: 0x4a6670,
    accent: 0x0d7a56,
    accent_hi: 0x0a6647,
    on_accent: 0xffffff,
    success: 0x1b6b3f,
    warn: 0x7a4f00,
    danger: 0xa3262c,
    dark: false,
    art: SkyArt {
        sun: [0xffffff, 0xeefaf6, 0xd5f1ea],
        far: 0xcfe3e6,
        near: 0xb2cbd6,
        cloud: [0xf6fbfa, 0xeef6f6, 0xe7f0f4],
        lit: [0xffffff; 3],
        shade: 0xd3e2e8,
        star: 0xffffff,
        mood: Mood::Aurora([0x5fe0a8, 0xa88cff, 0x6ad4f0]),
    },
};

/// Midnight Aurora: green and violet curtains over a deep blue night, on Horizon's glass pages.
pub const MIDNIGHT_AURORA: Theme = Theme {
    id: "midnight_aurora",
    name: "Midnight Aurora",
    sky: AURORA_DARK.sky,
    accent: ThemeAccent::Colour(0x4fe0a0),
    palette: Some((&AURORA_DARK, &AURORA_LIGHT)),
    look: Look::Painted,
};

/// The launcher themes past Horizon's own sky: the featured families first, then the older ones.
pub const THEMES: [Theme; 8] = [
    SUNRISE,
    HYPR_NEON,
    DEEP_OCEAN,
    AUTUMN_FOREST,
    CHERRY_BLOSSOM,
    MIDNIGHT_AURORA,
    FIRST_LIGHT,
    NORD,
];

/// Horizon's own theme's key in `Settings::theme_accents`.
pub const HORIZON: &str = "horizon";

/// A theme's key in `Settings::theme_accents`: its id, or [`HORIZON`].
pub fn accent_key(id: Option<&str>) -> &str {
    theme(id).map_or(HORIZON, |t| t.id)
}

/// The accent a theme starts with: the one last picked under it, else its own (violet for
/// Horizon's).
pub fn starting_accent(
    id: Option<&str>,
    kept: &std::collections::BTreeMap<String, Accent>,
) -> Accent {
    kept.get(accent_key(id))
        .copied()
        .unwrap_or_else(|| theme(id).map_or(Accent::VIOLET, |t| t.accent.accent()))
}

/// The theme named `id` (`Settings::theme`); None for Horizon's own sky or a name this launcher
/// doesn't know.
pub fn theme(id: Option<&str>) -> Option<&'static Theme> {
    THEMES.iter().find(|t| Some(t.id) == id)
}

/// The glass look's colour for each of [`Accent::ALL`]: the mockup's swatches.
pub const SWATCHES: [u32; 7] = [
    0x6b4bff, 0x2563eb, 0x0f766e, 0x15803d, 0xc2410c, 0xdc2626, 0xbe185d,
];

/// [`Accent::ALL`]'s names, in its order.
const NAMES: [&str; 7] = ["violet", "blue", "teal", "green", "amber", "red", "pink"];

impl Accent {
    pub const VIOLET: Accent = Accent {
        hue: None,
        rgb: None,
    };
    pub const BLUE: Accent = Accent {
        hue: Some(212.),
        rgb: None,
    };
    pub const TEAL: Accent = Accent {
        hue: Some(176.),
        rgb: None,
    };
    pub const GREEN: Accent = Accent {
        hue: Some(140.),
        rgb: None,
    };
    pub const AMBER: Accent = Accent {
        hue: Some(36.),
        rgb: None,
    };
    pub const RED: Accent = Accent {
        hue: Some(356.),
        rgb: None,
    };
    pub const PINK: Accent = Accent {
        hue: Some(322.),
        rgb: None,
    };

    pub const ALL: [Accent; 7] = [
        Self::VIOLET,
        Self::BLUE,
        Self::TEAL,
        Self::GREEN,
        Self::AMBER,
        Self::RED,
        Self::PINK,
    ];

    /// Any hue the player picks.
    pub fn custom(hue: f32) -> Self {
        Self {
            hue: Some(hue.rem_euclid(360.)),
            rgb: None,
        }
    }

    /// A colour (`0xrrggbb`): one of the swatches is its named accent.
    pub fn from_rgb(rgb: u32) -> Self {
        let rgb = rgb & 0xff_ffff;
        match SWATCHES.iter().position(|&c| c == rgb) {
            Some(i) => Self::ALL[i],
            None => Self {
                hue: Some(hsl(rgb).0),
                rgb: Some(rgb),
            },
        }
    }

    /// The colour the glass look wears: the one typed, a named accent's swatch, else the hue at
    /// the violet's saturation and lightness.
    pub fn rgb(self) -> u32 {
        if let Some(rgb) = self.rgb {
            return rgb;
        }
        match (Self::ALL.iter().position(|a| *a == self), self.hue) {
            (Some(i), _) => SWATCHES[i],
            (None, Some(hue)) => {
                let (_, s, l) = hsl(SWATCHES[0]);
                from_hsl(hue, s, l)
            }
            (None, None) => SWATCHES[0],
        }
    }

    /// [`rgb`](Self::rgb) as `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:06x}", self.rgb())
    }

    /// Its name in `settings.json` and on the command line: a named hue's, a typed colour's
    /// `#rrggbb`, else its degrees.
    pub fn name(self) -> String {
        if self.rgb.is_some() {
            return self.hex();
        }
        let named = Self::ALL
            .iter()
            .position(|a| *a == self)
            .map(|i| NAMES[i].to_owned());
        named.unwrap_or_else(|| match self.hue {
            None => "violet".to_owned(),
            Some(hue) => format!("{hue}"),
        })
    }

    /// A name, `#rrggbb`, or a hue in degrees.
    pub fn parse(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        if let Some(hex) = name.strip_prefix('#') {
            return (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| u32::from_str_radix(hex, 16).ok())
                .flatten()
                .map(Self::from_rgb);
        }
        let name = if name == "purple" || name == "default" {
            "violet"
        } else {
            &name
        };
        if let Some(i) = NAMES.iter().position(|n| *n == name) {
            return Some(Self::ALL[i]);
        }
        name.parse::<f32>()
            .ok()
            .filter(|hue| hue.is_finite())
            .map(Self::custom)
    }

    /// Its hue in degrees; None for the violet, whose tokens stay as they are.
    pub fn hue(self) -> Option<f32> {
        self.hue
    }

    /// `palette` with this accent.
    pub fn apply(self, mut p: Blocks) -> Blocks {
        let Some(hue) = self.hue else {
            return p;
        };
        let turn = |c: u32| {
            let (_, s, l) = hsl(c);
            (hue, s, l)
        };
        // The face holds its label: white on a dark face, Nord's ink on a light one. Move the
        // face away from the label until the two read.
        let step = if contrast(p.on_face, 0x000000) < contrast(p.on_face, 0xffffff) {
            1.
        } else {
            -1.
        };
        let face = fit(turn(p.face), |c| contrast(p.on_face, c) >= 4.5, step);
        let (_, _, face_l) = hsl(face);
        let (h, s, l) = turn(p.face_bevel);
        p.face_bevel = from_hsl(h, s, l.max(face_l + 0.08).min(0.92));
        let (h, s, l) = turn(p.face_lip);
        p.face_lip = from_hsl(h, s, l.min(face_l - 0.12).max(0.05));
        p.face = face;
        // The accent marks things on every background: lighter on dark, darker on light.
        let grounds = [p.page, p.raised, p.button, p.panel, p.well];
        let reads = |c: u32| grounds.iter().all(|&g| contrast(c, g) >= 3.2);
        p.accent = fit(turn(p.accent), reads, if p.dark { 1. } else { -1. });
        p
    }
}

impl TryFrom<String> for Accent {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::parse(&name).ok_or_else(|| format!("{name}: a colour name or a hue in degrees"))
    }
}

impl From<Accent> for String {
    fn from(accent: Accent) -> Self {
        accent.name()
    }
}

/// The colour the hue picker shows for `hue`: a strip of them, at one saturation and lightness.
pub fn hue_colour(hue: f32) -> u32 {
    from_hsl(hue.rem_euclid(360.), 0.62, 0.55)
}

/// The colour's hue (degrees), saturation and lightness (0 to 1).
fn hsl(c: u32) -> (f32, f32, f32) {
    let [r, g, b] = [16, 8, 0].map(|s| ((c >> s) & 255) as f32 / 255.);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.;
    if max == min {
        return (0., 0., l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6. } else { 0. }
    } else if max == g {
        (b - r) / d + 2.
    } else {
        (r - g) / d + 4.
    };
    (h * 60., s, l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> u32 {
    let c = (1. - (2. * l - 1.).abs()) * s;
    let x = c * (1. - ((h / 60.) % 2. - 1.).abs());
    let m = l - c / 2.;
    let (r, g, b) = match (h.rem_euclid(360.) / 60.) as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let byte = |v: f32| ((v + m).clamp(0., 1.) * 255.).round() as u32;
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

/// The colour, made lighter (`step` 1) or darker (-1) a little at a time until `good` holds.
fn fit((h, s, l): (f32, f32, f32), good: impl Fn(u32) -> bool, step: f32) -> u32 {
    let mut l = l;
    for _ in 0..100 {
        let c = from_hsl(h, s, l);
        if good(c) || !(0.0..=1.0).contains(&l) {
            return c;
        }
        l += step * 0.01;
    }
    from_hsl(h, s, l.clamp(0., 1.))
}

/// The pictures Home can show: what the decoders read.
const PICTURES: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// A picture above this is refused: a screen's worth is far less.
const MAX_BYTES: u64 = 40 << 20;

/// Keeps `file` as Home's background in `dir` (`background-<hash>.<ext>`: a new picture gets a
/// new name, so the window doesn't show the one it cached), after checking it is a picture, and
/// returns the kept file's name. An older background goes.
pub fn keep_background(dir: &Path, file: &Path) -> Result<String, String> {
    let shown = file.display();
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| PICTURES.contains(&e.as_str()))
        .ok_or_else(|| format!("{shown}: a PNG, JPEG or WebP picture, please."))?;
    let size = std::fs::metadata(file)
        .map_err(|e| format!("{shown}: {e}"))?
        .len();
    if size > MAX_BYTES {
        return Err(format!("{shown} is over 40 MB."));
    }
    let bytes = std::fs::read(file).map_err(|e| format!("{shown}: {e}"))?;
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .ok_or_else(|| format!("{shown} isn't a picture the launcher can read."))?;
    if w < 64 || h < 64 {
        return Err(format!("{shown} is {w}×{h}: too small to fill Home."));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let hash = crate::sources::sha1_hex(&bytes);
    let name = format!("background-{}.{ext}", &hash[..12]);
    std::fs::write(dir.join(&name), &bytes).map_err(|e| format!("{name}: {e}"))?;
    for old in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let other = old.file_name().to_string_lossy().into_owned();
        if other.starts_with("background-") && other != name {
            let _ = std::fs::remove_file(old.path());
        }
    }
    Ok(name)
}

/// Home's background, if the player keeps one and it is still there.
pub fn background(dir: &Path, name: Option<&str>) -> Option<PathBuf> {
    let name = name.filter(|n| n.starts_with("background-") && !n.contains(['/', '\\']))?;
    Some(dir.join(name)).filter(|p| p.is_file())
}

#[cfg(test)]
#[path = "themes/tests.rs"]
mod tests;
