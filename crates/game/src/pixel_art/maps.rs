// cspell:disable
//! Die ASCII-Pixelkarten und Paletten. Reines Rust, keine Bevy-Typen.
//!
//! Ein Zeichen = ein Pixel, `.` = durchsichtig. `K` ist überall die Kontur.

use super::canvas::Palette;

pub const OUTLINE: [u8; 4] = [18, 14, 16, 255];
pub const WHITE: [u8; 4] = [255, 250, 240, 255];

// --- Herz --------------------------------------------------------------------

pub const HEART: &[&str] = &[
    "..KKK.KKK..",
    ".KRRRKRRRK.",
    "KRWRRRRRRRK",
    "KRWRRRRRRRK",
    "KRRRRRRRRRK",
    ".KRRRRRRRK.",
    "..KRRRRRK..",
    "...KRRRK...",
    "....KRK....",
    ".....K.....",
];

pub const HEART_FULL: Palette = &[('K', OUTLINE), ('R', [220, 30, 45, 255]), ('W', WHITE)];
/// Gleiche Karte, aber Füllung dunkel: ein leeres Herz.
pub const HEART_EMPTY: Palette = &[
    ('K', OUTLINE),
    ('R', [70, 25, 32, 255]),
    ('W', [95, 45, 52, 255]),
];

// --- Münze, Schlüssel, Bombe, Item, Falltür ------------------------------------

pub const COIN: &[&str] = &[
    "..KKKK..", ".KYYYYK.", "KYWYYYYK", "KYWYYOYK", "KYYYYOYK", "KYYYYOYK", ".KYOOOK.", "..KKKK..",
];

pub const COIN_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('Y', [245, 200, 50, 255]),
    ('O', [185, 130, 20, 255]),
    ('W', WHITE),
];

pub const KEY: &[&str] = &[
    ".KKK.......",
    "KGGGKKKKKKK",
    "KG.GGGGGGGK",
    "KGGGKKKGKGK",
    ".KKK...K.K.",
];

pub const KEY_PALETTE: Palette = &[('K', OUTLINE), ('G', [205, 210, 225, 255])];

pub const BOMB: &[&str] = &[
    "......Y..",
    ".....O.Y.",
    "....K....",
    "..KKKKK..",
    ".KBBBBBK.",
    "KBWBBBBBK",
    "KBWBBBBBK",
    "KBBBBBBBK",
    ".KBBBBBK.",
    "..KKKKK..",
];

pub const BOMB_PALETTE: Palette = &[
    ('K', OUTLINE),
    // Mittelgrau statt Schwarz: sonst verschwindet die Bombe auf dem dunklen Boden.
    ('B', [100, 100, 118, 255]),
    ('W', [190, 190, 205, 255]),
    ('O', [240, 140, 30, 255]),
    ('Y', [255, 230, 90, 255]),
];

/// Falltür (Holzrahmen, dunkles Loch) – mit anderer Palette der goldene Ausgang.
pub const TRAPDOOR: &[&str] = &[
    ".KKKKKKKKKKKK.",
    "KWWWWWWWWWWWWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWHHHHHHHHHHWK",
    "KWWWWWWWWWWWWK",
    ".KKKKKKKKKKKK.",
];

pub const TRAPDOOR_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('W', [125, 82, 45, 255]),
    ('H', [6, 5, 8, 255]),
];

pub const EXIT_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('W', [235, 195, 60, 255]),
    ('H', [255, 245, 190, 255]),
];

/// Edelstein über einem Steinsockel.
pub const ITEM: &[&str] = &[
    ".....KK.....",
    "....KCCK....",
    "...KCWCCK...",
    "..KCWCCCCK..",
    "...KCCCCK...",
    "....KCCK....",
    ".....KK.....",
    "............",
    ".KKKKKKKKKK.",
    ".KSSSSSSSSK.",
    "..KSSSSSSK..",
    "..KKKKKKKK..",
];

pub const ITEM_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('C', [80, 225, 235, 255]),
    ('W', WHITE),
    ('S', [140, 132, 125, 255]),
];

// --- Figuren -------------------------------------------------------------------
//
// Alle Figuren schauen nach rechts. Nach links wird das Sprite gespiegelt
// (`Sprite::flip_x`) – eine Karte reicht für beide Richtungen.

/// Der Spieler: kleiner Wanderer mit Kapuze.
pub const PLAYER: &[&str] = &[
    "....KKKKKK....",
    "...KHHHHHHK...",
    "..KHHHHHHHLK..",
    ".KHHHSSSSSHLK.",
    ".KHHSSSSSSSHK.",
    ".KHSSSWESSWEK.",
    ".KHSSSEESSEEK.",
    ".KHSSSSSSSSSK.",
    "..KHSSSSSMMK..",
    "..KHHSSSSSHK..",
    ".KCCKHHHHHKCK.",
    ".KCCCCCCCCCCK.",
    "..KCCCCCCCCK..",
    "...KKK..KKK...",
];

pub const PLAYER_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('H', [70, 120, 130, 255]),
    ('L', [110, 165, 170, 255]),
    ('S', [240, 205, 170, 255]),
    ('E', [25, 20, 30, 255]),
    ('W', WHITE),
    ('M', [170, 90, 90, 255]),
    ('C', [50, 85, 95, 255]),
];

/// Chaser: roter Klumpen mit Zähnen.
pub const CHASER: &[&str] = &[
    "...KKKKKK...",
    "..KRRRRRRK..",
    ".KRLRRRRRRK.",
    "KRLRRRRWERRK",
    "KRRRRRREERRK",
    "KRRRRRRRRRRK",
    "KRRKKKKKKKRK",
    "KRRKTKTKTKRK",
    "KRRKKKKKKKRK",
    ".KRRRRRRRRK.",
    ".KDRDRRDRDK.",
    "..KK.KK.KK..",
];

pub const CHASER_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('R', [200, 60, 55, 255]),
    ('L', [235, 120, 105, 255]),
    ('D', [130, 35, 35, 255]),
    ('W', WHITE),
    ('E', [20, 10, 10, 255]),
    ('T', [245, 240, 225, 255]),
];

/// Shooter: schwebendes Auge in einem orangen Panzer.
pub const SHOOTER: &[&str] = &[
    "...KKKKKK...",
    "..KOOOOOOK..",
    ".KOLKKKKOOK.",
    "KOLKWWWWKOOK",
    "KOKWWWWBBKOK",
    "KOKWWWBPPKOK",
    "KOKWWWBPPKOK",
    "KOKWWWWBBKOK",
    "KOOKWWWWKOOK",
    ".KOOKKKKOOK.",
    "..KDOOOODK..",
    "...KKKKKK...",
];

pub const SHOOTER_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('O', [225, 140, 45, 255]),
    ('L', [250, 195, 110, 255]),
    ('D', [160, 90, 25, 255]),
    ('W', [245, 240, 230, 255]),
    ('B', [60, 150, 90, 255]),
    ('P', [15, 15, 20, 255]),
];

/// Charger: blauer Käfer mit Horn nach vorn (rechts).
pub const CHARGER: &[&str] = &[
    "............",
    "..KKKKKK....",
    ".KBBBBBBK...",
    "KBLLBBBBBKK.",
    "KBLBBBBBWEHK",
    "KBBBBBBBEEHK",
    "KBBBBBBBBBK.",
    "KDBDBBDBBBK.",
    ".KDDDDDDDK..",
    "..KK.KK.KK..",
    "............",
    "............",
];

pub const CHARGER_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('B', [75, 95, 200, 255]),
    ('L', [140, 160, 240, 255]),
    ('D', [45, 55, 130, 255]),
    ('W', WHITE),
    ('E', [15, 15, 25, 255]),
    ('H', [230, 225, 205, 255]),
];

/// Boss: großer violetter Hornschädel.
pub const BOSS: &[&str] = &[
    ".KK..............KK.",
    "KHHK............KHHK",
    "KHHK....KKKK....KHHK",
    ".KHHK.KKPPPPKK.KHHK.",
    "..KHHKPPPPPPPPKHHK..",
    "...KKPLLPPPPPPPKK...",
    "...KPLLPPPPPPPPPK...",
    "..KPLPPPPPPPPPPPPK..",
    "..KPPKKKPPPPKKKPPK..",
    ".KPPKYYEKPPKYYEKPPK.",
    ".KPPKYEEKPPKYEEKPPK.",
    ".KPPPKKKPPPPKKKPPPK.",
    ".KPPPPPPPPPPPPPPPPK.",
    ".KPPPPPKKKKKKPPPPPK.",
    ".KPPPPKTKTKTKKPPPPK.",
    "..KPPPKKKKKKKKPPPK..",
    "..KDPPPPPPPPPPPPDK..",
    "...KDDPPDPPDPPDDK...",
    "....KKDDKDDKDDKK....",
    "......KK.KK.KK......",
];

pub const BOSS_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('P', [140, 50, 130, 255]),
    ('L', [195, 110, 185, 255]),
    ('D', [85, 25, 80, 255]),
    ('H', [225, 215, 190, 255]),
    ('Y', [255, 220, 70, 255]),
    ('E', [30, 10, 10, 255]),
    ('T', [245, 240, 225, 255]),
];

/// Schuss (Träne des Spielers bzw. Kugel der Gegner – andere Palette).
pub const SHOT: &[&str] = &["..KK..", ".KWCK.", "KWCCCK", "KCCCCK", ".KCDK.", "..KK.."];

pub const TEAR_PALETTE: Palette = &[
    ('K', [25, 45, 80, 255]),
    ('W', WHITE),
    ('C', [120, 180, 240, 255]),
    ('D', [70, 120, 200, 255]),
];

pub const ENEMY_SHOT_PALETTE: Palette = &[
    ('K', [70, 10, 15, 255]),
    ('W', [255, 200, 180, 255]),
    ('C', [235, 70, 60, 255]),
    ('D', [170, 35, 35, 255]),
];

// --- Kachel-Deko ----------------------------------------------------------------

/// Felsbrocken, wird auf eine Bodenkachel gemalt.
pub const ROCK: &[&str] = &[
    "....KKKKK.....",
    "..KKLLLGGKK...",
    ".KLLLGGGGGGK..",
    ".KLGGGGGGGGGK.",
    "KLGGGGGGDGGGK.",
    "KLGGGGGDGGGGGK",
    "KGGGGGDGGGGGGK",
    "KGGGGGGGGGGGDK",
    "KDGGGGGGGGGDDK",
    ".KDGGGGGGGDDK.",
    ".KDDDGGGDDDDK.",
    "..KKDDDDDDKK..",
    "....KKKKKK....",
];

pub const ROCK_PALETTE: Palette = &[
    ('K', OUTLINE),
    ('L', [170, 162, 150, 255]),
    ('G', [128, 120, 112, 255]),
    ('D', [88, 82, 78, 255]),
];

/// Schlüsselloch für die verschlossene Tür.
pub const KEYHOLE: &[&str] = &[".KK.", "KEEK", "KEEK", ".KK.", ".EE.", ".EE.", "KKKK"];

pub const KEYHOLE_PALETTE: Palette = &[('K', [120, 85, 20, 255]), ('E', [20, 15, 10, 255])];
