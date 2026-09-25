//! Gibt eine Etage als ASCII-Karte aus.
//!
//! ```text
//! cargo run -p dungeon_gen --example print_floor
//! cargo run -p dungeon_gen --example print_floor -- 1A2B-3C4D-5E6F-7A8B 3
//! ```
//!
//! Argumente (beide optional): Seed, Etagentiefe.

use dungeon_gen::{RoomKind, RunSeed, floor};

fn main() {
    let mut args = std::env::args().skip(1);

    let seed = match args.next() {
        Some(text) => text.parse::<RunSeed>().unwrap_or_else(|e| {
            eprintln!("Ungültiger Seed '{text}': {e}");
            std::process::exit(1);
        }),
        None => RunSeed::from_entropy(),
    };
    let depth: u32 = args.next().and_then(|d| d.parse().ok()).unwrap_or(1);

    let floor = floor::generate(seed, depth);

    println!("Seed {seed}, Etage {depth}, {} Räume\n", floor.len());
    print!("{}", floor.to_ascii());
    println!("\nS Start  B Boss  T Schatz  $ Shop  # normal\n");

    for (pos, room) in floor.rooms() {
        if room.kind != RoomKind::Normal {
            println!(
                "{:>9} bei ({:>2}, {:>2})  Entfernung {}  Vorlage '{}'",
                // Abgeleitetes `Debug` ignoriert Breitenangaben, daher erst in einen String.
                format!("{:?}", room.kind),
                pos.x,
                pos.y,
                room.distance,
                room.template.name
            );
        }
    }
}
