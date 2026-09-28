//! Beute auf dem Boden: Pickups, Raum-Belohnungen, Shop-Angebot, Schlüsselregeln.

use crate::{
    GridPos, RoomKind,
    floor::FLOOR_SIZE,
    items::{ItemDb, ItemId, ItemPools, Pool},
    rng::{Rng, RunSeed},
};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PickupKind {
    HalfHeart,
    Heart,
    Coin,
    Key,
    Bomb,
}

/// Was auf einer Kachel liegen kann.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Loot {
    Pickup(PickupKind),
    /// Item auf einem Sockel – kostenlos.
    Item(ItemId),
    /// Ware im Shop.
    ForSale {
        ware: Ware,
        price: u32,
    },
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Ware {
    Pickup(PickupKind),
    Item(ItemId),
}

/// Startausrüstung eines Runs.
pub const START_KEYS: u32 = 1;
pub const START_BOMBS: u32 = 1;

/// Eigener Stream je Etage, Raum und Zweck.
pub fn room_rng(seed: RunSeed, purpose: &str, depth: u32, room: GridPos) -> Rng {
    let room_index = (room.y * FLOOR_SIZE + room.x) as u64;
    seed.stream(purpose, u64::from(depth) * 1_000 + room_index)
}

/// Belohnung nach dem Räumen eines normalen Raums (oder nichts).
pub fn room_clear_drop(rng: &mut Rng) -> Option<PickupKind> {
    const TABLE: [(Option<PickupKind>, u32); 6] = [
        (None, 40),
        (Some(PickupKind::Coin), 25),
        (Some(PickupKind::HalfHeart), 12),
        (Some(PickupKind::Heart), 6),
        (Some(PickupKind::Key), 9),
        (Some(PickupKind::Bomb), 8),
    ];
    let weights = TABLE.map(|(_, w)| w);
    TABLE[rng.weighted_index(&weights)].0
}

/// Muss man einen Schlüssel benutzen, um diesen Raum zu betreten?
/// Der Shop immer, der Schatzraum ab Etage 2.
pub fn requires_key(kind: RoomKind, depth: u32) -> bool {
    match kind {
        RoomKind::Shop => true,
        RoomKind::Treasure => depth >= 2,
        _ => false,
    }
}

/// Die Waren im Shop: zwei Items, ein Herz, ein Schlüssel oder eine Bombe.
pub fn shop_stock(db: &ItemDb, pools: &mut ItemPools, rng: &mut Rng) -> Vec<Ware> {
    let utility = if rng.chance(0.5) {
        PickupKind::Key
    } else {
        PickupKind::Bomb
    };
    vec![
        Ware::Item(pools.draw(db, Pool::Shop, rng)),
        Ware::Pickup(PickupKind::Heart),
        Ware::Pickup(utility),
        Ware::Item(pools.draw(db, Pool::Shop, rng)),
    ]
}

pub fn price(db: &ItemDb, ware: &Ware) -> u32 {
    match ware {
        // Unbekanntes Item (aus der Datei gelöscht): Standardpreis.
        Ware::Item(id) => db.get(id).map_or(10, |i| i.price),
        Ware::Pickup(PickupKind::Heart) => 3,
        Ware::Pickup(PickupKind::HalfHeart) => 2,
        Ware::Pickup(PickupKind::Key | PickupKind::Bomb) => 5,
        Ware::Pickup(PickupKind::Coin) => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn drop_table_roughly_matches_weights() {
        let mut rng = Rng::from_seed(11);
        let mut counts: BTreeMap<Option<PickupKind>, u32> = BTreeMap::new();
        for _ in 0..10_000 {
            *counts.entry(room_clear_drop(&mut rng)).or_default() += 1;
        }
        let nothing = counts[&None];
        assert!((3_700..4_300).contains(&nothing), "{counts:?}");
        assert!(counts.contains_key(&Some(PickupKind::Key)));
    }

    #[test]
    fn key_rules() {
        assert!(requires_key(RoomKind::Shop, 1));
        assert!(!requires_key(RoomKind::Treasure, 1));
        assert!(requires_key(RoomKind::Treasure, 2));
        assert!(!requires_key(RoomKind::Normal, 5));
        assert!(!requires_key(RoomKind::Boss, 5));
    }

    #[test]
    fn shop_has_two_different_items_and_sane_prices() {
        for s in 0..200 {
            let db = ItemDb::builtin();
            let mut pools = ItemPools::new(&db, &[]);
            let stock = shop_stock(&db, &mut pools, &mut Rng::from_seed(s));
            assert_eq!(stock.len(), 4);
            let items: Vec<_> = stock
                .iter()
                .filter_map(|w| {
                    if let Ware::Item(i) = w {
                        Some(i.clone())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(items.len(), 2);
            assert_ne!(items[0], items[1]);
            assert!(stock.iter().all(|w| (1..=30).contains(&price(&db, w))));
        }
    }
}
