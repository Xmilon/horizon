//! Content mods in the resource root (docs/modpacks.md, registries 0.2).
//!
//! Every loader reads one resource root, and most of its block and item
//! tables are exports from vanilla indexed by block-state id or keyed by
//! name. A content mod's block copies a vanilla *template*. So the mods'
//! root is an overlay, like the resource-pack one:
//! - the base root linked, with the per-state and per-name tables
//!   rewritten: each mod block gets its template's rows appended (states
//!   after vanilla's, mods and blocks in id order) with names, translation
//!   keys and in-table state ids moved over, and the declared overrides
//!   applied;
//! - each mod's own `assets/` and `data/` files;
//! - additions to tags merged into the tag files and bundles.
//!
//! The loaders need no mod code, vanilla state ids never move, and with no
//! content mod the root is the base root itself.
use crate::{Result, blocks::MAP_COLORS};
use horizon_core::geometry::{Aabb, Axis, ShapeOp, Vec3, VoxelShape};
use horizon_mods::content::{Block, Content, MobEffect, Rarity};
use horizon_world::nbt::{self, JavaString, Limits, Tag};
use serde::{Deserialize, Serialize, de};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    ops::Range,
    path::{Path, PathBuf},
};

/// A content mod to put in the root: its id and content.
pub struct ContentMod {
    pub id: String,
    pub content: Content,
}

/// The mods' mob effects in registry order: by id, as items are. Their
/// network ids follow vanilla's in this order.
pub fn mob_effects(mods: &[ContentMod]) -> Vec<&MobEffect> {
    let mut effects: Vec<&MobEffect> = mods.iter().flat_map(|m| &m.content.effects).collect();
    effects.sort_by(|a, b| a.id.cmp(&b.id));
    effects
}

/// JSON with its object keys in file order, so a rewritten table differs
/// from the original only by what was added.
#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}
impl<'de> Deserialize<'de> for J {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = J;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> std::result::Result<J, E> {
                Ok(J::Null)
            }
            fn visit_bool<E>(self, v: bool) -> std::result::Result<J, E> {
                Ok(J::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<J, E> {
                serde_json::Number::from_f64(v)
                    .map(J::Num)
                    .ok_or_else(|| E::custom("not a JSON number"))
            }
            fn visit_str<E>(self, v: &str) -> std::result::Result<J, E> {
                Ok(J::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> std::result::Result<J, E> {
                Ok(J::Str(v))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(J::Arr(out))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(kv) = a.next_entry()? {
                    out.push(kv);
                }
                Ok(J::Obj(out))
            }
        }
        d.deserialize_any(V)
    }
}
impl Serialize for J {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(t) => s.serialize_str(t),
            J::Arr(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            J::Obj(o) => {
                let mut map = s.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}
impl J {
    fn remap_state_ids(&mut self, from: u32, to: u32) {
        match self {
            J::Num(n) if n.as_u64() == Some(from as u64) => *n = to.into(),
            J::Arr(a) => a.iter_mut().for_each(|v| v.remap_state_ids(from, to)),
            J::Obj(o) => o.iter_mut().for_each(|(_, v)| v.remap_state_ids(from, to)),
            _ => {}
        }
    }
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn remove(&mut self, key: &str) {
        if let J::Obj(o) = self {
            o.retain(|(k, _)| k != key);
        }
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Obj(o) => o.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// Sets `key`, in place if present, else at the end.
    fn set(&mut self, key: &str, value: J) {
        if let Some(v) = self.get_mut(key) {
            *v = value;
        } else if let J::Obj(o) = self {
            o.push((key.to_owned(), value));
        }
    }
    /// The value at a `/`-separated path ("" is the document).
    fn at(&mut self, path: &str) -> Option<&mut J> {
        path.split('/')
            .filter(|p| !p.is_empty())
            .try_fold(self, |j, key| j.get_mut(key))
    }
    fn f64(v: f64) -> J {
        serde_json::Number::from_f64(v).map_or(J::Null, J::Num)
    }
    fn as_u32(&self) -> Option<u32> {
        match self {
            J::Num(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
            _ => None,
        }
    }
    /// Renames strings and, with `ids`, moves state ids, everywhere inside.
    fn rename(&mut self, r: &Rename, ids: bool) {
        match self {
            J::Str(s) => {
                if let Some(new) = r.string(s) {
                    *s = new;
                }
            }
            J::Num(n) if ids => {
                if let Some(new) = n
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .and_then(|id| r.id(id))
                {
                    *n = new.into();
                }
            }
            J::Arr(a) => a.iter_mut().for_each(|v| v.rename(r, ids)),
            J::Obj(o) => o.iter_mut().for_each(|(_, v)| v.rename(r, ids)),
            _ => {}
        }
    }
}

/// One id moving to another: `minecraft:iron_block` → `ruby:ruby_block`,
/// with its state range for blocks.
#[derive(Clone)]
struct Rename {
    from: String,
    to: String,
    states: Range<u32>,
    first: u32,
}
impl Rename {
    fn new(from: &str, to: &str) -> Self {
        Rename {
            from: from.to_owned(),
            to: to.to_owned(),
            states: 0..0,
            first: 0,
        }
    }
    /// The id itself, `Block{id}`, and `block.<ns>.<path>` / `item.<ns>.<path>` keys.
    fn string(&self, s: &str) -> Option<String> {
        if s == self.from {
            return Some(self.to.clone());
        }
        if s.strip_prefix("Block{").and_then(|x| x.strip_suffix('}')) == Some(&self.from) {
            return Some(format!("Block{{{}}}", self.to));
        }
        let key = |id: &str| id.replacen(':', ".", 1).replace('/', ".");
        for kind in ["block", "item"] {
            if s.strip_prefix(kind).and_then(|x| x.strip_prefix('.')) == Some(&key(&self.from)) {
                return Some(format!("{kind}.{}", key(&self.to)));
            }
        }
        None
    }
    fn id(&self, id: u32) -> Option<u32> {
        self.states
            .contains(&id)
            .then(|| self.first + (id - self.states.start))
    }
}

// DiscreteVoxelShape.forAllEdges: merge consecutive boundary edges.
fn shape_edges(shape: &VoxelShape) -> Vec<J> {
    let sizes = shape.sizes();
    let mut result = Vec::new();
    for axis in [2, 0, 1] {
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        for u in 0..=sizes[a] {
            for v in 0..=sizes[b] {
                let mut start = None;
                for t in 0..=sizes[axis] {
                    let mut count = 0;
                    let mut odd = 0;
                    for du in 0..=1 {
                        for dv in 0..=1 {
                            let mut p = [0i32; 3];
                            p[axis] = t as i32;
                            p[a] = u as i32 + du - 1;
                            p[b] = v as i32 + dv - 1;
                            if shape.is_full(p[0], p[1], p[2]) {
                                count += 1;
                                odd ^= du ^ dv;
                            }
                        }
                    }
                    let edge = count == 1 || count == 3 || (count == 2 && odd == 0);
                    if edge {
                        if start.is_none() {
                            start = Some(t);
                        }
                    } else if let Some(first) = start.take() {
                        let mut from = [0.0; 3];
                        let mut to = [0.0; 3];
                        from[a] = shape.coordinates(Axis::ALL[a])[u];
                        from[b] = shape.coordinates(Axis::ALL[b])[v];
                        to[a] = from[a];
                        to[b] = from[b];
                        from[axis] = shape.coordinates(Axis::ALL[axis])[first];
                        to[axis] = shape.coordinates(Axis::ALL[axis])[t];
                        result.push(J::Arr(from.into_iter().chain(to).map(J::f64).collect()));
                    }
                }
            }
        }
    }
    result
}

fn encode_shape(shape: &VoxelShape) -> Tag {
    let mut c = nbt::Compound::new();
    let (min, max) = shape.discrete_bounds();
    c.insert("canonical".into(), Tag::Byte(0));
    c.insert("cube".into(), Tag::Byte(i8::from(shape.is_cube())));
    let axes = shape.cube_coordinate_axes();
    c.insert(
        "cube_axes".into(),
        Tag::Byte((axes[0] as i8) | ((axes[1] as i8) << 1) | ((axes[2] as i8) << 2)),
    );
    for (name, axis) in ["x", "y", "z"].into_iter().zip(Axis::ALL) {
        c.insert(
            name.into(),
            Tag::LongArray(
                shape
                    .coordinates(axis)
                    .iter()
                    .map(|n| n.to_bits() as i64)
                    .collect(),
            ),
        );
    }
    let [x, y, z] = shape.sizes();
    let mut occupied = Vec::with_capacity(x * y * z);
    for i in 0..x {
        for j in 0..y {
            for k in 0..z {
                occupied.push(u8::from(shape.is_full(i as i32, j as i32, k as i32)));
            }
        }
    }
    c.insert("occupied".into(), Tag::ByteArray(occupied));
    c.insert("discrete_min".into(), Tag::IntArray(min.to_vec()));
    c.insert("discrete_max".into(), Tag::IntArray(max.to_vec()));
    Tag::Compound(c)
}

/// A block being added: its declaration and where its states go.
struct NewBlock<'a> {
    decl: &'a Block,
    rename: Rename,
    /// The template's item, if it has one.
    item: Option<String>,
    shapes: Option<Vec<VoxelShape>>,
    default: u32,
    source_states: Vec<u32>,
    properties: Option<Vec<BTreeMap<String, String>>>,
}
/// An item being added (declared, or a block's).
struct NewItem {
    rename: Rename,
    max_stack_size: Option<u8>,
    rarity: Option<Rarity>,
    components: Vec<(String, J)>,
    crafting_remainder: Option<String>,
}

struct Plan<'a> {
    blocks: Vec<NewBlock<'a>>,
    items: Vec<NewItem>,
    effects: Vec<&'a MobEffect>,
    /// Vanilla's state count (the tables' length before).
    states: usize,
    /// Each vanilla block's default state.
    defaults: BTreeMap<String, u32>,
    /// The gameplay rows' sound objects, by block, for `sounds` overrides.
    sounds: BTreeMap<String, J>,
}

/// An item's component patch on its entry; `known` holds every type a vanilla item has, so typos fail.
fn patch_components(
    entry: &mut J,
    patch: &[(String, J)],
    known: &BTreeSet<String>,
) -> std::result::Result<(), String> {
    for (key, value) in patch {
        let kind = key.strip_prefix('!').unwrap_or(key);
        if !known.contains(kind) {
            return Err(format!("{kind} is no component a vanilla item has"));
        }
        if kind == key {
            entry.set(key, value.clone());
        } else {
            entry.remove(kind);
        }
    }
    Ok(())
}

/// A declared component patch, as the overlay's JSON.
fn component_patch(
    id: &str,
    components: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<(String, J)>> {
    Ok(components
        .iter()
        .map(|(k, v)| Ok((k.clone(), serde_json::from_value(v.clone())?)))
        .collect::<std::result::Result<_, serde_json::Error>>()
        .map_err(|e| format!("{id}: {e}"))?)
}

fn read_json(path: &Path) -> Result<J> {
    Ok(serde_json::from_slice(&fs::read(path)?).map_err(|e| format!("{}: {e}", path.display()))?)
}

impl<'a> Plan<'a> {
    fn new(base: &Path, mods: &'a [ContentMod]) -> Result<Self> {
        let mut facts = read_json(&base.join("facts.json"))?;
        let names: Vec<String> = match facts.at("states") {
            Some(J::Arr(states)) => states
                .iter()
                .map(|s| match s {
                    J::Arr(row) => match row.first() {
                        Some(J::Str(name)) => Ok(name.clone()),
                        _ => Err("facts.json: a state without a name"),
                    },
                    _ => Err("facts.json: a state that is not a row"),
                })
                .collect::<std::result::Result<_, _>>()?,
            _ => return Err("facts.json: no states".into()),
        };
        let defaults: BTreeMap<String, u32> = match facts.at("defaults") {
            Some(J::Obj(o)) => o
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_u32()?)))
                .collect(),
            _ => return Err("facts.json: no defaults".into()),
        };
        let range = |name: &str| -> Option<Range<u32>> {
            let start = names.iter().position(|n| n == name)?;
            let len = names[start..].iter().take_while(|n| *n == name).count();
            // A block's states are one run in registry order.
            (names[start + len..].iter().all(|n| n != name))
                .then_some(start as u32..(start + len) as u32)
        };
        let mut blocks = Vec::new();
        let mut next = names.len() as u32;
        let mut sorted: Vec<&ContentMod> = mods.iter().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        let mut wanted_sounds = Vec::new();
        for m in &sorted {
            let mut declared: Vec<&Block> = m.content.blocks.iter().collect();
            declared.sort_by(|a, b| a.id.cmp(&b.id));
            for decl in declared {
                if defaults.contains_key(&decl.id) {
                    return Err(format!("{}: {} is already a block", m.id, decl.id).into());
                }
                let states = range(&decl.template).ok_or_else(|| {
                    format!("{}: template {} is not a block", decl.id, decl.template)
                })?;
                let native_states = decl.native.as_ref().map(|n| n.states()).transpose()?;
                let source_states = if let Some(states_list) = &native_states {
                    vec![states.start; states_list.len()]
                } else {
                    states.clone().collect()
                };
                let shapes = if let Some(native) = &decl.native {
                    let mut shape = VoxelShape::empty();
                    for b in &native.collision {
                        let part = VoxelShape::from_box(Aabb::new(
                            Vec3::new(b[0], b[1], b[2]),
                            Vec3::new(b[3], b[4], b[5]),
                        ))?;
                        shape = shape.join_unoptimized(&part, ShapeOp::OR)?;
                    }
                    Some(vec![shape; source_states.len()])
                } else if let Some(spec) = &decl.shapes {
                    let Some(J::Arr(rows)) = facts.get("states") else {
                        unreachable!()
                    };
                    let mut used = BTreeSet::new();
                    let mut out = Vec::new();
                    for id in states.clone() {
                        let J::Arr(row) = &rows[id as usize] else {
                            unreachable!()
                        };
                        let Some(J::Str(value)) = row
                            .get(4)
                            .and_then(|x| x.get("properties"))
                            .and_then(|x| x.get(&spec.property))
                        else {
                            return Err(format!(
                                "{}: template state lacks property {}",
                                decl.id, spec.property
                            )
                            .into());
                        };
                        let boxes = spec.values.get(value).ok_or_else(|| {
                            format!("{}: no shape for {}={value}", decl.id, spec.property)
                        })?;
                        used.insert(value.as_str());
                        let mut shape = VoxelShape::empty();
                        for b in boxes {
                            let part = VoxelShape::from_box(Aabb::new(
                                Vec3::new(b[0], b[1], b[2]),
                                Vec3::new(b[3], b[4], b[5]),
                            ))?;
                            shape = shape.join_unoptimized(&part, ShapeOp::OR)?;
                        }
                        out.push(shape);
                    }
                    if used.len() != spec.values.len() {
                        return Err(format!(
                            "{}: shape values include an unknown {} value",
                            decl.id, spec.property
                        )
                        .into());
                    }
                    Some(out)
                } else {
                    None
                };
                let mut default = *defaults.get(&decl.template).ok_or_else(|| {
                    format!(
                        "{}: template {} has no default state",
                        decl.id, decl.template
                    )
                })?;
                if !decl.default_state.is_empty() {
                    let Some(J::Arr(rows)) = facts.get("states") else {
                        unreachable!()
                    };
                    let properties = |id: u32| -> Option<&J> {
                        let J::Arr(row) = rows.get(id as usize)? else {
                            return None;
                        };
                        row.get(4)?.get("properties")
                    };
                    let mut wanted = properties(default).cloned().ok_or_else(|| {
                        format!("{}: template default has no state properties", decl.id)
                    })?;
                    for (key, value) in &decl.default_state {
                        if wanted.get(key).is_none() {
                            return Err(format!(
                                "{}: unknown state property {key} on {}",
                                decl.id, decl.template
                            )
                            .into());
                        }
                        wanted.set(key, J::Str(value.clone()));
                    }
                    let matches: Vec<u32> = states
                        .clone()
                        .filter(|id| {
                            properties(*id).is_some_and(|p| {
                                let (J::Obj(a), J::Obj(b)) = (p, &wanted) else {
                                    return false;
                                };
                                a.len() == b.len() && b.iter().all(|(k, v)| p.get(k) == Some(v))
                            })
                        })
                        .collect();
                    if matches.len() != 1 {
                        return Err(format!(
                            "{}: default_state {:?} matches {} states of {}",
                            decl.id,
                            decl.default_state,
                            matches.len(),
                            decl.template
                        )
                        .into());
                    }
                    default = matches[0];
                }
                let mut rename = Rename::new(&decl.template, &decl.id);
                rename.first = next;
                next += source_states.len() as u32;
                default = if let Some(properties) = &native_states {
                    properties
                        .iter()
                        .position(|p| *p == decl.native.as_ref().unwrap().default_state)
                        .ok_or("native block has no default state")? as u32
                } else {
                    default - states.start
                };
                rename.states = states;
                if let Some(s) = &decl.properties.sounds {
                    if !defaults.contains_key(s) {
                        return Err(format!("{}: sounds {s} is not a block", decl.id).into());
                    }
                    wanted_sounds.push(s.clone());
                }
                blocks.push(NewBlock {
                    decl,
                    rename,
                    item: None,
                    shapes,
                    default,
                    source_states,
                    properties: native_states,
                });
            }
        }
        if next > MAX_STATES {
            let mut by_mod: BTreeMap<&str, u32> = BTreeMap::new();
            for b in &blocks {
                *by_mod
                    .entry(&b.decl.id[..b.decl.id.find(':').unwrap_or(0)])
                    .or_default() += b.source_states.len() as u32;
            }
            let list: Vec<String> = by_mod.iter().map(|(m, n)| format!("{m} {n}")).collect();
            return Err(format!(
                "{next} block states, more than the {MAX_STATES} a world can hold ({} in the game; \
                 the mods: {})",
                names.len(),
                list.join(", ")
            )
            .into());
        }
        // Block items copy the template block's item.
        let gameplay = read_json(&base.join("gameplay-facts.json"))?;
        let rows = match gameplay.get("states") {
            Some(J::Arr(rows)) if rows.len() == names.len() => rows,
            _ => return Err("gameplay-facts.json does not match facts.json".into()),
        };
        let mut sounds = BTreeMap::new();
        for s in wanted_sounds {
            let row = &rows[defaults[&s] as usize];
            sounds.insert(s, row.get("sound").cloned().unwrap_or(J::Null));
        }
        let mut items = Vec::new();
        for b in &mut blocks {
            let row = &rows[b.rename.states.start as usize];
            b.item = match row.get("item") {
                Some(J::Str(i)) if i != "minecraft:air" => Some(i.clone()),
                _ => None,
            };
            if b.decl.item {
                let item = b.item.as_deref().ok_or_else(|| {
                    format!("{}: template {} has no item", b.decl.id, b.decl.template)
                })?;
                items.push(NewItem {
                    rename: Rename::new(item, &b.decl.id),
                    max_stack_size: None,
                    rarity: None,
                    components: component_patch(&b.decl.id, &b.decl.components)?,
                    crafting_remainder: None,
                });
            }
        }
        for m in &sorted {
            for i in &m.content.items {
                items.push(NewItem {
                    rename: Rename::new(&i.template, &i.id),
                    max_stack_size: i.max_stack_size,
                    rarity: i.rarity,
                    components: component_patch(&i.id, &i.components)?,
                    crafting_remainder: i.crafting_remainder.clone(),
                });
            }
        }
        // Block items and standalone items share the same registry. Sort
        // them together by mod ID and entry name before assigning IDs.
        items.sort_by(|a, b| a.rename.to.cmp(&b.rename.to));
        Ok(Plan {
            blocks,
            items,
            effects: mob_effects(mods),
            states: names.len(),
            defaults,
            sounds,
        })
    }

    /// What moves in a new block's copies of its template's rows: the
    /// block, and the template's item when it has another name. A block
    /// without an item gets `minecraft:air` where rows name one.
    fn renames(&self, b: &NewBlock) -> Vec<Rename> {
        let mut out = vec![b.rename.clone()];
        if let Some(item) = b.item.as_deref().filter(|i| *i != b.decl.template)
            && b.decl.item
        {
            out.push(Rename::new(item, &b.decl.id));
        }
        out
    }

    /// Appends each new block's copies of the template's states to `list`;
    /// `gameplay` rows get their index and the declared overrides.
    fn states(&self, list: &mut Vec<J>, ids: bool, gameplay: bool, identities: bool) -> Result<()> {
        if list.len() != self.states {
            return Err(format!("{} states, not {}", list.len(), self.states).into());
        }
        for b in &self.blocks {
            let renames = self.renames(b);
            for (i, &id) in b.source_states.iter().enumerate() {
                let mut row = list[id as usize].clone();
                for r in &renames {
                    row.rename(r, ids);
                }
                if let Some(properties) = &b.properties {
                    if identities && let J::Arr(facts) = &mut row {
                        if let Some(state) = facts.get_mut(4) {
                            *state = serde_json::from_value(
                                serde_json::json!({"id": b.decl.id, "properties": properties[i]}),
                            )?;
                        }
                    } else if row.get("properties").is_some() {
                        row.set(
                            "properties",
                            serde_json::from_value(serde_json::to_value(&properties[i])?)?,
                        );
                    }
                    if ids {
                        row.remap_state_ids(b.rename.first, b.rename.first + i as u32);
                    }
                }
                if gameplay {
                    row.set("id", J::Num((b.rename.first + i as u32).into()));
                    if !b.decl.item {
                        row.set("item", J::Str("minecraft:air".into()));
                    }
                    self.override_gameplay(b, &mut row);
                    if let Some(native) = &b.decl.native {
                        row.set("block_entity", J::Bool(native.block_entity));
                        if native.collision != [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]] {
                            row.set("can_occlude", J::Bool(false));
                            row.set("shape_occlusion", J::Bool(false));
                            row.set("solid_render", J::Bool(false));
                        }
                    }
                    if let Some(shapes) = &b.shapes {
                        let shape = &shapes[i];
                        let boxes = shape
                            .boxes()
                            .iter()
                            .map(|b| {
                                J::Arr(
                                    [
                                        b.min(Axis::X),
                                        b.min(Axis::Y),
                                        b.min(Axis::Z),
                                        b.max(Axis::X),
                                        b.max(Axis::Y),
                                        b.max(Axis::Z),
                                    ]
                                    .into_iter()
                                    .map(J::f64)
                                    .collect(),
                                )
                            })
                            .collect();
                        row.set("outline", J::Arr(boxes));
                        row.set("outline_edges", J::Arr(shape_edges(shape)));
                        row.set(
                            "full_collision",
                            J::Bool(!shape.join_is_not_empty(&VoxelShape::block(), ShapeOp::XOR)?),
                        );
                    }
                }
                list.push(row);
            }
        }
        Ok(())
    }

    /// The declared properties, on a gameplay row.
    fn override_gameplay(&self, b: &NewBlock, row: &mut J) {
        let p = &b.decl.properties;
        if let Some(v) = p.hardness {
            row.set("hardness", J::f64(v as f64));
        }
        if let Some(v) = p.explosion_resistance {
            row.set("explosion_resistance", J::f64(v as f64));
        }
        if let Some(v) = p.light {
            row.set("emission", J::Num(v.into()));
        }
        if let Some(v) = p.requires_tool {
            row.set("requires_tool", J::Bool(v));
        }
        if let Some(v) = p.friction {
            row.set("friction", J::f64(v as f64));
        }
        if let Some(v) = p.map_color {
            let col = if v == 0 { 0 } else { MAP_COLORS[v as usize] };
            row.set("map_color", J::Num(col.into()));
        }
        if let Some(s) = &p.sounds {
            row.set("sound", self.sounds[s].clone());
        }
    }

    /// Adds each new block's copy of the template's entry to a map by name.
    fn names(&self, map: &mut J, ids: bool) {
        for b in &self.blocks {
            if let Some(v) = map.get(&b.decl.template) {
                let mut v = v.clone();
                for r in &self.renames(b) {
                    v.rename(r, ids);
                }
                map.set(&b.decl.id, v);
            }
        }
    }

    /// Adds each new item's copy of its template's entry to a map by name;
    /// `components` applies the overrides (items.json).
    fn items(&self, map: &mut J, components: bool) -> Result<()> {
        let known: BTreeSet<String> = match map {
            J::Obj(entries) if components => entries
                .iter()
                .filter_map(|(_, v)| match v {
                    J::Obj(c) => Some(c.iter().map(|(k, _)| k.clone())),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => BTreeSet::new(),
        };
        for i in &self.items {
            let Some(v) = map.get(&i.rename.from) else {
                continue;
            };
            let mut v = v.clone();
            v.rename(&i.rename, false);
            if components {
                if let Some(n) = i.max_stack_size {
                    v.set("minecraft:max_stack_size", J::Num(n.into()));
                }
                if let Some(r) = i.rarity {
                    let r = match r {
                        Rarity::Common => "common",
                        Rarity::Uncommon => "uncommon",
                        Rarity::Rare => "rare",
                        Rarity::Epic => "epic",
                    };
                    v.set("minecraft:rarity", J::Str(r.into()));
                }
                patch_components(&mut v, &i.components, &known)
                    .map_err(|e| format!("{}: {e}", i.rename.to))?;
            }
            map.set(&i.rename.to, v);
        }
        if components {
            for i in &self.items {
                if let Some(rest) = &i.crafting_remainder
                    && map.get(rest).is_none()
                {
                    return Err(format!(
                        "{}: crafting_remainder {rest:?} is not an item",
                        i.rename.to
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    /// One JSON table, rewritten.
    fn json(&self, name: &str, doc: &mut J) -> Result<()> {
        let states = |doc: &mut J, path: &str, ids: bool, gameplay: bool| -> Result<()> {
            match doc.at(path) {
                Some(J::Arr(a)) => self.states(a, ids, gameplay, name == "facts.json"),
                _ => Err(format!("no list at /{path}").into()),
            }
        };
        match name {
            "facts.json" => {
                states(doc, "states", false, false)?;
                let defaults = doc.at("defaults").ok_or("no defaults")?;
                for b in &self.blocks {
                    let id = b.rename.first + b.default;
                    defaults.set(&b.decl.id, J::Num(id.into()));
                }
            }
            "gameplay-facts.json" => states(doc, "states", false, true)?,
            "block-behaviours.json" => {
                states(doc, "states", false, false)?;
                let Some(J::Arr(blocks)) = doc.at("blocks") else {
                    return Err("no blocks".into());
                };
                for b in &self.blocks {
                    let template = blocks.iter().find(
                        |x| matches!(x.get("name"), Some(J::Str(n)) if *n == b.decl.template),
                    );
                    if let Some(mut v) = template.cloned() {
                        for r in &self.renames(b) {
                            v.rename(r, false);
                        }
                        if !b.decl.item {
                            v.set("item", J::Str("minecraft:air".into()));
                        }
                        blocks.push(v);
                    }
                }
                self.items(doc.at("items").ok_or("no items")?, false)?;
                for odds in ["fire/burnOdds", "fire/igniteOdds"] {
                    if let Some(map) = doc.at(odds) {
                        self.names(map, false);
                    }
                }
            }
            "model-states.json" => {
                self.names(doc, false);
                for b in &self.blocks {
                    if let Some(properties) = &b.properties {
                        let native = b.decl.native.as_ref().unwrap();
                        let schema: BTreeMap<_, _> = native
                            .properties
                            .iter()
                            .map(|(k, v)| (k, serde_json::json!({"named":v})))
                            .collect();
                        doc.set(
                            &b.decl.id,
                            serde_json::from_value(
                                serde_json::json!({"schema":schema,"states":properties}),
                            )?,
                        );
                    }
                }
            }
            "landing-definitions.json" | "worldgen/blocks/survival-rules.json" => {
                self.names(doc, false)
            }
            "render-definitions.json" => {
                self.names(doc, true);
                for b in &self.blocks {
                    if b.properties.is_some()
                        && let Some(entry) = doc.get_mut(&b.decl.id)
                    {
                        entry.set("default", J::Num((b.rename.first + b.default).into()));
                        entry.set(
                            "states",
                            J::Arr(
                                (0..b.source_states.len())
                                    .map(|i| J::Num((b.rename.first + i as u32).into()))
                                    .collect(),
                            ),
                        );
                        if b.decl.native.as_ref().unwrap().collision
                            != [[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]
                        {
                            entry.set("can_occlude", J::Bool(false));
                        }
                    }
                }
            }
            "worldgen/blocks/survival-extra-definitions.json" => {
                if let Some(map) = doc.at("ignition_odds") {
                    self.names(map, false);
                }
            }
            "items.json" => self.items(doc, true)?,
            "registries.json" => {
                // Network ids: the game and its server map items, blocks and mob
                // effects through this report, so new ones get the ids after
                // vanilla's, in plan order.
                let new_items = self.items.iter().map(|i| i.rename.to.clone());
                let new_blocks = self.blocks.iter().map(|b| b.decl.id.clone());
                let mut registries = vec![
                    ("minecraft:item", new_items.collect::<Vec<_>>()),
                    ("minecraft:block", new_blocks.collect()),
                ];
                if !self.effects.is_empty() {
                    let new_effects = self.effects.iter().map(|e| e.id.clone());
                    registries.push(("minecraft:mob_effect", new_effects.collect()));
                }
                for (registry, names) in registries {
                    let Some(J::Obj(entries)) =
                        doc.get_mut(registry).and_then(|r| r.get_mut("entries"))
                    else {
                        return Err(format!("no {registry} entries").into());
                    };
                    let next = entries
                        .iter()
                        .filter_map(|(_, e)| match e {
                            J::Obj(o) => o.iter().find(|(k, _)| k == "protocol_id")?.1.as_u32(),
                            _ => None,
                        })
                        .max()
                        .map_or(0, |m| m + 1);
                    for (i, name) in names.into_iter().enumerate() {
                        if entries.iter().any(|(k, _)| *k == name) {
                            continue;
                        }
                        let id = J::Num((next + i as u32).into());
                        entries.push((name, J::Obj(vec![("protocol_id".into(), id)])));
                    }
                }
            }
            "command-registries.json" => {
                self.names(doc.at("blocks").ok_or("no blocks")?, false);
                for b in &self.blocks {
                    if let Some(native) = &b.decl.native {
                        let schema: Vec<_> = native
                            .properties
                            .iter()
                            .map(|(k, v)| serde_json::json!([k, v]))
                            .collect();
                        doc.at("blocks").unwrap().set(
                            &b.decl.id,
                            serde_json::from_value(serde_json::to_value(schema)?)?,
                        );
                    }
                }
                let new_items = || self.items.iter().map(|i| J::Str(i.rename.to.clone()));
                let new_blocks = || self.blocks.iter().map(|b| J::Str(b.decl.id.clone()));
                let Some(J::Arr(items)) = doc.at("items") else {
                    return Err("no items".into());
                };
                items.extend(new_items());
                // The registries commands check ids against.
                if let Some(J::Arr(ids)) = doc.at("by_registry/minecraft:item/ids") {
                    ids.extend(new_items());
                }
                if let Some(J::Arr(ids)) = doc.at("by_registry/minecraft:block/ids") {
                    ids.extend(new_blocks());
                }
                // `/effect` and the mob_effect resource arguments.
                let new_effects = || self.effects.iter().map(|e| J::Str(e.id.clone()));
                if let Some(J::Arr(ids)) = doc.at("effects") {
                    ids.extend(new_effects());
                }
                if let Some(J::Arr(ids)) = doc.at("by_registry/minecraft:mob_effect/ids") {
                    ids.extend(new_effects());
                }
            }
            "mob-data.json" => states(doc, "states", false, false)?,
            "worldgen/block_flags.json" => states(doc, "", false, false)?,
            "worldgen/blocks/structure-blocks.json" => {
                for key in ["rotate", "mirror", "full_block"] {
                    states(doc, key, true, false)?;
                }
                let added: usize = self.blocks.iter().map(|b| b.source_states.len()).sum();
                doc.set("states", J::Num(((self.states + added) as u64).into()));
            }
            _ => return Err("no rule for this table".into()),
        }
        Ok(())
    }

    fn collision_shapes(&self, tag: &mut Tag) -> Result<()> {
        let Tag::Compound(root) = tag else {
            return Err("collision table is not a compound".into());
        };
        let mut indices = Vec::new();
        let Some(Tag::List(entries)) = root.get_mut(&JavaString::from("shapes")) else {
            return Err("collision table has no shapes".into());
        };
        for block in &self.blocks {
            if let Some(shapes) = &block.shapes {
                for (i, shape) in shapes.iter().enumerate() {
                    let id = entries.len() as i32;
                    entries.push(encode_shape(shape));
                    indices.push((block.rename.first as usize + i, id));
                }
            }
        }
        let Some(Tag::IntArray(mapping)) = root.get_mut(&JavaString::from("cached_shapes")) else {
            return Err("collision table has no mapping".into());
        };
        for (state, id) in indices {
            mapping[state] = id;
        }
        Ok(())
    }

    /// One NBT table: `states` lists, `cached_shapes` arrays and `blocks`
    /// compounds, wherever they are.
    fn nbt(&self, tag: &mut Tag) {
        let Tag::Compound(c) = tag else {
            return;
        };
        for (key, value) in c.iter_mut() {
            match (key.to_string_lossy().as_str(), value) {
                ("states", Tag::List(list)) if list.len() == self.states => {
                    for b in &self.blocks {
                        for (i, &id) in b.source_states.iter().enumerate() {
                            let mut state = list[id as usize].clone();
                            if let Some(slot) = state.get_mut("id") {
                                *slot = Tag::String(b.decl.id.as_str().into());
                            }
                            if let Some(properties) = &b.properties {
                                if matches!(state, Tag::String(_)) {
                                    state = Tag::Compound(nbt::Compound::new());
                                }
                                if let Tag::Compound(c) = &mut state {
                                    c.insert("id".into(), Tag::String(b.decl.id.as_str().into()));
                                    c.insert(
                                        "properties".into(),
                                        Tag::Compound(
                                            properties[i]
                                                .iter()
                                                .map(|(k, v)| {
                                                    (
                                                        k.as_str().into(),
                                                        Tag::String(v.as_str().into()),
                                                    )
                                                })
                                                .collect(),
                                        ),
                                    );
                                }
                            }
                            list.push(state);
                        }
                    }
                }
                ("cached_shapes", Tag::IntArray(a)) if a.len() == self.states => {
                    for b in &self.blocks {
                        let copy: Vec<i32> =
                            b.source_states.iter().map(|id| a[*id as usize]).collect();
                        a.extend(copy);
                    }
                }
                ("blocks", Tag::Compound(map)) => {
                    for b in &self.blocks {
                        let Some(mut v) = map
                            .get(&JavaString::from(b.decl.template.as_str()))
                            .cloned()
                        else {
                            continue;
                        };
                        if let (Some(f), Some(slot)) =
                            (b.decl.properties.friction, v.get_mut("friction"))
                        {
                            *slot = Tag::Float(f.to_bits());
                        }
                        map.insert(b.decl.id.as_str().into(), v);
                    }
                }
                (_, inner @ Tag::Compound(_)) => self.nbt(inner),
                _ => {}
            }
        }
    }
}

/// The JSON and NBT tables rewritten for new blocks and items.
const JSON_TABLES: [&str; 14] = [
    "facts.json",
    "registries.json",
    "gameplay-facts.json",
    "block-behaviours.json",
    "model-states.json",
    "render-definitions.json",
    "landing-definitions.json",
    "items.json",
    "command-registries.json",
    "mob-data.json",
    "worldgen/block_flags.json",
    "worldgen/blocks/structure-blocks.json",
    "worldgen/blocks/survival-rules.json",
    "worldgen/blocks/survival-extra-definitions.json",
];
const NBT_TABLES: [&str; 6] = [
    "occlusion.nbt",
    "cached-collision.nbt",
    "movement-properties.nbt",
    "worldgen/blocks/block-properties.nbt",
    "worldgen/blocks/cached-support.nbt",
    "worldgen/blocks/cached-collision.nbt",
];
use horizon_items::content_namespaces::MARKER;
pub use horizon_items::content_namespaces::{namespaces, tag_namespaces};
/// Block states a world can hold. Chunk sections widen their global palette
/// past vanilla's 16 bits to fit the registry (crate::chunk::set_block_states);
/// this caps it at 20 bits, about 29 times vanilla's 35,723 states. Mods past
/// it are refused, not wrapped onto other blocks.
pub const MAX_STATES: u32 = 1 << 20;

/// Bundles of block tag files (tag id → the tag file's text).
const TAG_BUNDLES: [&str; 2] = ["block-tags.json", "worldgen/block_tags.json"];
/// The worldgen loader's bundle of biome tags.
const BIOME_TAGS: &str = "worldgen/biome_tags.json";
/// Worldgen registries a content mod may add to, and the bundle the
/// worldgen loader reads each from (id → the file's text): what structure
/// mods hold. Biomes, noise and density functions change the terrain and
/// are not read from mods.
const WORLDGEN_BUNDLES: [(&str, &str); 6] = [
    ("structure", "worldgen/structures.json"),
    ("structure_set", "worldgen/structure_sets.json"),
    ("template_pool", "worldgen/template_pools.json"),
    ("processor_list", "worldgen/processor_lists.json"),
    ("configured_feature", "worldgen/features.json"),
    ("placed_feature", "worldgen/placed_features.json"),
];

/// The registry of `command-registries.json` that each bundle's ids join,
/// for the commands that suggest them (`/locate`, `/place`).
const COMMAND_REGISTRIES: [(&str, &str); 6] = [
    ("worldgen/structures.json", "minecraft:worldgen/structure"),
    (
        "worldgen/structure_sets.json",
        "minecraft:worldgen/structure_set",
    ),
    (
        "worldgen/template_pools.json",
        "minecraft:worldgen/template_pool",
    ),
    (
        "worldgen/processor_lists.json",
        "minecraft:worldgen/processor_list",
    ),
    ("worldgen/features.json", "minecraft:worldgen/feature"),
    (
        "worldgen/placed_features.json",
        "minecraft:worldgen/placed_feature",
    ),
];

/// A mod file that adds to a worldgen bundle: (bundle, id).
fn worldgen_entry(file: &str) -> Option<(&'static str, String)> {
    let (ns, rest) = file.strip_prefix("data/")?.split_once("/worldgen/")?;
    let (kind, path) = rest.split_once('/')?;
    let (_, bundle) = WORLDGEN_BUNDLES.iter().find(|(k, _)| *k == kind)?;
    Some((bundle, format!("{ns}:{}", path.strip_suffix(".json")?)))
}

/// The key of a mod's structure set, template pool or processor list file
/// that adds its entries to other sets, pools or lists (vanilla's, another
/// mod's) instead of being one of its own. Vanilla data never has it.
const ADD_TO: &str = "horizon:add_to";
/// An addition's place among the others (lower first; 1000 by default), as
/// Lithostitched's modifiers' `priority`.
const PRIORITY: &str = "horizon:priority";
/// The bundles a mod may add to, and the list each entry goes into.
const ADDABLE: [(&str, &str); 3] = [
    ("worldgen/structure_sets.json", "structures"),
    ("worldgen/template_pools.json", "elements"),
    ("worldgen/processor_lists.json", "processors"),
];

/// A mod file's additions (`horizon:add_to`): its entries appended to the
/// list of each set, pool or processor list it names, as Lithostitched's
/// `add_structure_set_entries`, `add_template_pool_elements` and
/// `add_processor_list_processors` do for NeoForge and Fabric packs.
struct Addition {
    targets: Vec<String>,
    list: &'static str,
    entries: Vec<J>,
    priority: i64,
}
impl Addition {
    /// What a mod file of `bundle` adds, if it is an addition file.
    fn read(bundle: &str, text: &str) -> Result<Option<Self>> {
        let Some((_, list)) = ADDABLE.iter().find(|(b, _)| *b == bundle) else {
            return Ok(None);
        };
        if !text.contains(ADD_TO) {
            return Ok(None);
        }
        let doc: J = serde_json::from_str(text)?;
        let Some(targets) = doc.get(ADD_TO) else {
            return Ok(None);
        };
        let targets = match targets {
            J::Str(s) => vec![s.clone()],
            J::Arr(a) => a
                .iter()
                .map(|v| match v {
                    J::Str(s) => Some(s.clone()),
                    _ => None,
                })
                .collect::<Option<_>>()
                .ok_or(format!("{ADD_TO}: not a list of ids"))?,
            _ => return Err(format!("{ADD_TO}: not an id or a list of ids").into()),
        };
        let Some(J::Arr(entries)) = doc.get(list) else {
            return Err(format!("{ADD_TO} without a {list} list").into());
        };
        let priority = match doc.get(PRIORITY) {
            None => 1000,
            Some(J::Num(n)) => n.as_i64().ok_or(format!("{PRIORITY}: not an integer"))?,
            Some(_) => return Err(format!("{PRIORITY}: not an integer").into()),
        };
        Ok(Some(Self {
            targets,
            list,
            entries: entries.clone(),
            priority,
        }))
    }
    /// Appends the entries to each target in the bundle `doc` (id → the
    /// file's text). A target nobody defines is skipped, as a tag entry
    /// naming an unknown id is.
    fn apply(&self, doc: &mut J) -> Result<()> {
        for target in &self.targets {
            let Some(J::Str(text)) = doc.get(target) else {
                continue;
            };
            let mut v: J = serde_json::from_str(text).map_err(|e| format!("{target}: {e}"))?;
            let Some(J::Arr(list)) = v.get_mut(self.list) else {
                continue;
            };
            list.extend(self.entries.iter().cloned());
            doc.set(target, J::Str(serde_json::to_string(&v)?));
        }
        Ok(())
    }
}

/// The creative inventory's tabs (`creative.rs`).
const CREATIVE: &str = "creative-inventory.json";
/// Content mods' tabs per page: the top row's five category columns and the
/// bottom row's six; the saved toolbars, search and inventory are on every page.
const TABS_PER_PAGE: usize = 11;

/// A mod's creative tab file, `data/<ns>/horizon/creative_tab/<name>.json`:
/// a tab of its own, `<ns>:<name>`, or with `horizon:add_to` its items for
/// another tab (vanilla's, another mod's).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TabFile {
    title: Option<String>,
    icon: Option<String>,
    items: Vec<String>,
    #[serde(rename = "horizon:add_to")]
    add_to: Option<String>,
}
struct CreativeTab {
    id: String,
    file: TabFile,
}

/// An item stack as `creative-inventory.json` writes one.
fn stack_snbt(item: &str) -> J {
    J::Str(format!("{{count:1,id:\"{item}\"}}"))
}

/// The mods' tabs into the creative inventory `doc`: each tab of its own on
/// the pages after vanilla's, eleven a page in the mods' order, then the
/// additions to tabs, then every item of the mods' tabs in the search tab
/// (searched by its English `names`). An item nobody has is left out.
fn creative_tabs(
    doc: &mut J,
    tabs: &[CreativeTab],
    known: &Known,
    names: &BTreeMap<String, String>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let Some(J::Arr(list)) = doc.get_mut("tabs") else {
        return Err(format!("{CREATIVE}: no tabs").into());
    };
    fn keep(tab: &CreativeTab, known: &Known, warnings: &mut Vec<String>) -> Vec<String> {
        let (have, gone): (Vec<String>, Vec<String>) = tab
            .file
            .items
            .iter()
            .cloned()
            .partition(|i| known.items.contains(i));
        if !gone.is_empty() {
            warnings.push(format!(
                "creative tab {}: {} are no items; left out",
                tab.id,
                gone.join(", ")
            ));
        }
        have
    }
    let mut searched: Vec<String> = Vec::new();
    let own = tabs.iter().filter(|t| t.file.add_to.is_none());
    for (i, tab) in own.enumerate() {
        let items = keep(tab, known, warnings);
        let icon = tab
            .file
            .icon
            .clone()
            .or_else(|| items.first().cloned())
            .unwrap_or_else(|| "minecraft:stone".into());
        let slot = i % TABS_PER_PAGE;
        let (row, column) = if slot < 5 {
            ("TOP", slot)
        } else {
            ("BOTTOM", slot - 5)
        };
        let title = tab.file.title.clone().unwrap_or_else(|| tab.id.clone());
        searched.extend(items.iter().cloned());
        list.push(J::Obj(vec![
            ("id".into(), J::Str(tab.id.clone())),
            ("type".into(), J::Str("CATEGORY".into())),
            ("row".into(), J::Str(row.into())),
            ("column".into(), J::Num((column as u64).into())),
            (
                "page".into(),
                J::Num(((1 + i / TABS_PER_PAGE) as u64).into()),
            ),
            ("title".into(), J::Str(title)),
            ("icon".into(), stack_snbt(&icon)),
            ("visible".into(), J::Bool(true)),
            (
                "items".into(),
                J::Arr(items.iter().map(|i| stack_snbt(i)).collect()),
            ),
        ]));
    }
    for tab in tabs.iter().filter(|t| t.file.add_to.is_some()) {
        let target = tab.file.add_to.as_deref().unwrap_or_default();
        let items = keep(tab, known, warnings);
        let found = list
            .iter_mut()
            .find(|t| t.get("id") == Some(&J::Str(target.into())));
        let Some(J::Arr(into)) = found.and_then(|t| t.get_mut("items")) else {
            warnings.push(format!(
                "creative tab {}: no tab {target} to add to",
                tab.id
            ));
            continue;
        };
        into.extend(items.iter().map(|i| stack_snbt(i)));
        searched.extend(items);
    }
    let mut seen = std::collections::BTreeSet::new();
    searched.retain(|i| seen.insert(i.clone()));
    let search = list
        .iter_mut()
        .find(|t| t.get("type") == Some(&J::Str("SEARCH".into())));
    if let Some(search) = search {
        // Items, their tooltip lines and tags go by index: keep them in step.
        let name = |item: &str| {
            let (ns, path) = item.split_once(':').unwrap_or(("minecraft", item));
            names
                .get(&format!("item.{ns}.{path}"))
                .or_else(|| names.get(&format!("block.{ns}.{path}")))
                .cloned()
                .unwrap_or_else(|| path.replace('_', " "))
                .to_lowercase()
        };
        let lines = search.get("lines").is_some();
        let tags = search.get("tags").is_some();
        for item in &searched {
            if let Some(J::Arr(items)) = search.get_mut("items") {
                items.push(stack_snbt(item));
            }
            if lines && let Some(J::Arr(l)) = search.get_mut("lines") {
                l.push(J::Arr(vec![J::Str(name(item))]));
            }
            if tags && let Some(J::Arr(t)) = search.get_mut("tags") {
                t.push(J::Arr(Vec::new()));
            }
        }
    }
    Ok(())
}

/// The worldgen loader's bundle of biomes (id -> the biome file's text).
const BIOMES: &str = "worldgen/biomes.json";
/// Where a mod keeps NeoForge's biome modifiers, under `data/<ns>/`.
const BIOME_MODIFIERS: &str = "/neoforge/biome_modifier/";
/// `GenerationStep.Decoration` in order: the index of a biome's `features` list.
const STEPS: [&str; 11] = [
    "raw_generation",
    "lakes",
    "local_modifications",
    "underground_structures",
    "surface_structures",
    "strongholds",
    "underground_ores",
    "underground_decoration",
    "fluid_springs",
    "vegetal_decoration",
    "top_layer_modification",
];

/// A biome modifier of type `neoforge:add_features`: its placed features
/// appended to a decoration step of each biome it names, as NeoForge does.
/// What a mod needs to put its ore in vanilla's biomes, whose files it may
/// not change.
struct AddFeatures {
    file: String,
    /// Biome ids and `#` biome tags.
    biomes: Vec<String>,
    features: Vec<String>,
    step: usize,
}
impl AddFeatures {
    /// A mod file's modifier; `Ok(None)` for another modifier type, which is
    /// not supported yet.
    fn read(file: &str, text: &str) -> Result<Option<Self>> {
        let doc: J = serde_json::from_str(text).map_err(|e| format!("{file}: {e}"))?;
        match doc.get("type") {
            Some(J::Str(t)) if t == "neoforge:add_features" => {}
            Some(J::Str(_)) => return Ok(None),
            _ => return Err(format!("{file}: no type").into()),
        }
        let ids = |key: &str| -> Result<Vec<String>> {
            let list = match doc.get(key) {
                Some(J::Str(s)) => return Ok(vec![s.clone()]),
                Some(J::Arr(a)) => a,
                _ => {
                    return Err(format!("{file}: {key} is an id, a #tag or a list of ids").into());
                }
            };
            list.iter()
                .map(|v| match v {
                    J::Str(s) => Ok(s.clone()),
                    _ => Err(format!("{file}: {key} holds ids").into()),
                })
                .collect()
        };
        let step = match doc.get("step") {
            Some(J::Str(s)) => STEPS
                .iter()
                .position(|n| n == s)
                .ok_or_else(|| format!("{file}: step {s:?} is one of {}", STEPS.join(", ")))?,
            _ => return Err(format!("{file}: no step").into()),
        };
        Ok(Some(Self {
            file: file.to_owned(),
            biomes: ids("biomes")?,
            features: ids("features")?,
            step,
        }))
    }
}

/// The biome ids `selector` names: an id, or every biome of a `#` tag in
/// `tags` (the biome tag bundle: id -> the tag file's text), tags in tags
/// included.
fn biome_ids(selector: &str, tags: &J, out: &mut Vec<String>, depth: u32) {
    let Some(tag) = selector.strip_prefix('#') else {
        if !out.iter().any(|b| b == selector) {
            out.push(selector.to_owned());
        }
        return;
    };
    let Some(J::Str(text)) = tags.get(tag) else {
        return;
    };
    let Ok(doc) = serde_json::from_str::<J>(text) else {
        return;
    };
    let Some(J::Arr(values)) = doc.get("values") else {
        return;
    };
    for v in values {
        let id = match v {
            J::Str(s) => s.as_str(),
            J::Obj(_) => match v.get("id") {
                Some(J::Str(s)) => s.as_str(),
                _ => continue,
            },
            _ => continue,
        };
        if depth < 16 {
            biome_ids(id, tags, out, depth + 1);
        }
    }
}

/// Applies `adds` to the biome bundle `biomes`. A biome nobody has is
/// skipped (another mod's, a tag entry naming one); a placed feature nobody
/// defines in `placed` fails, as vanilla fails to load such a biome.
fn add_features(biomes: &mut J, tags: &J, placed: &J, adds: &[AddFeatures]) -> Result<()> {
    for add in adds {
        if let Some(f) = add.features.iter().find(|f| placed.get(f).is_none()) {
            return Err(format!("{}: placed feature {f} is not defined", add.file).into());
        }
        let mut ids = Vec::new();
        for selector in &add.biomes {
            biome_ids(selector, tags, &mut ids, 0);
        }
        for id in ids {
            let Some(J::Str(text)) = biomes.get(&id) else {
                continue;
            };
            let mut biome: J = serde_json::from_str(text).map_err(|e| format!("{id}: {e}"))?;
            let Some(J::Arr(steps)) = biome.get_mut("features") else {
                return Err(format!("{id}: no features list").into());
            };
            while steps.len() <= add.step {
                steps.push(J::Arr(Vec::new()));
            }
            let step = &mut steps[add.step];
            // A step that is one feature becomes a list; a feature tag stays as it is.
            if let J::Str(one) = step
                && !one.starts_with('#')
            {
                *step = J::Arr(vec![J::Str(one.clone())]);
            }
            let J::Arr(list) = step else {
                return Err(
                    format!("{}: step {} of {id} is a feature tag", add.file, add.step).into(),
                );
            };
            for f in &add.features {
                let f = J::Str(f.clone());
                if !list.contains(&f) {
                    list.push(f);
                }
            }
            biomes.set(&id, J::Str(serde_json::to_string(&biome)?));
        }
    }
    Ok(())
}

/// The overlay being written: `out` mirrors `base` through links, and a
/// directory becomes real only when something inside it is written.
struct Overlay<'a> {
    base: &'a Path,
    out: &'a Path,
}
impl Overlay<'_> {
    fn new<'a>(base: &'a Path, out: &'a Path) -> Result<Overlay<'a>> {
        if fs::symlink_metadata(out).is_ok() {
            fs::remove_dir_all(out)?;
        }
        fs::create_dir_all(out)?;
        let overlay = Overlay { base, out };
        overlay.link_children(Path::new(""))?;
        Ok(overlay)
    }
    fn link_children(&self, rel: &Path) -> Result<()> {
        let Ok(read) = fs::read_dir(self.base.join(rel)) else {
            return Ok(());
        };
        for entry in read.flatten() {
            let rel = rel.join(entry.file_name());
            link(&self.base.join(&rel), &self.out.join(&rel))?;
        }
        Ok(())
    }
    /// Makes `rel`'s directories real, each holding links to the base's.
    fn dir(&self, rel: &Path) -> Result<()> {
        let mut at = PathBuf::new();
        for part in rel.iter() {
            at.push(part);
            let path = self.out.join(&at);
            let meta = fs::symlink_metadata(&path);
            if meta
                .as_ref()
                .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            {
                continue;
            }
            if meta.is_ok() {
                fs::remove_file(&path)?;
            }
            fs::create_dir(&path)?;
            self.link_children(&at)?;
        }
        Ok(())
    }
    fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let rel = Path::new(rel);
        if let Some(parent) = rel.parent() {
            self.dir(parent)?;
        }
        let path = self.out.join(rel);
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_file(&path)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }
}
fn out_path(overlay: &Overlay<'_>, rel: &str) -> PathBuf {
    overlay.out.join(rel)
}
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}
#[cfg(not(unix))]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        fs::create_dir_all(at)?;
        for entry in fs::read_dir(target)?.flatten() {
            link(&entry.path(), &at.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        // A hard link costs no time or space and needs no rights (symbolic
        // links do on Windows); copying the whole base root took minutes on a
        // first start. Another volume gets a copy. `Overlay::write` replaces a
        // file instead of writing into it, so the base is never changed.
        fs::hard_link(target, at).or_else(|_| fs::copy(target, at).map(|_| ()))
    }
}

/// A tag file's JSON text with `more` appended to its values (no repeats).
fn merge_tag(text: Option<&str>, more: &[J], what: &str) -> Result<String> {
    let mut doc = match text {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("{what}: {e}"))?,
        None => J::Obj(vec![("values".into(), J::Arr(Vec::new()))]),
    };
    let Some(J::Arr(values)) = doc.get_mut("values") else {
        return Err(format!("{what}: no values").into());
    };
    for v in more {
        if !values.contains(v) {
            values.push(v.clone());
        }
    }
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// The block and item ids and tags a content root has, for tag entries.
struct Known {
    blocks: std::collections::BTreeSet<String>,
    items: std::collections::BTreeSet<String>,
    /// The worldgen loader's biomes and biome tags.
    biomes: std::collections::BTreeSet<String>,
    biome_tags: std::collections::BTreeSet<String>,
    /// `data/<ns>/tags/<kind>/<path>.json` files the mods add.
    tag_files: std::collections::BTreeSet<String>,
}
impl Known {
    fn new(base: &Path, plan: &Plan, tags: &BTreeMap<String, TagMerge>) -> Result<Self> {
        let mut blocks: std::collections::BTreeSet<String> =
            plan.defaults.keys().cloned().collect();
        blocks.extend(plan.blocks.iter().map(|b| b.decl.id.clone()));
        let mut items = std::collections::BTreeSet::new();
        if let J::Obj(o) = read_json(&base.join("items.json"))? {
            items.extend(o.into_iter().map(|(k, _)| k));
        }
        items.extend(plan.items.iter().map(|i| i.rename.to.clone()));
        let keys = |name: &str| -> Result<std::collections::BTreeSet<String>> {
            Ok(match base.join(name).exists() {
                true => match read_json(&base.join(name))? {
                    J::Obj(o) => o.into_iter().map(|(k, _)| k).collect(),
                    _ => Default::default(),
                },
                false => Default::default(),
            })
        };
        Ok(Known {
            blocks,
            items,
            biomes: keys("worldgen/biomes.json")?,
            biome_tags: keys(BIOME_TAGS)?,
            tag_files: tags.keys().cloned().collect(),
        })
    }
    /// Whether tag `file` keeps entry `v`: a known id or an existing tag,
    /// for block, item and biome tags (other registries are kept as they
    /// are). The worldgen loader refuses a biome tag naming what it lacks
    /// (another mod's biomes, a loader's tags), so those entries go.
    fn keeps(&self, file: &str, v: &J) -> bool {
        let id = match v {
            J::Str(s) => s.as_str(),
            J::Obj(_) => match v.get("id") {
                Some(J::Str(s)) => s.as_str(),
                _ => return true,
            },
            _ => return true,
        };
        let Some(rest) = file
            .strip_prefix("data/")
            .and_then(|r| r.split_once("/tags/"))
            .map(|(_, rest)| rest)
        else {
            return true;
        };
        let kind = match rest.strip_prefix("worldgen/biome/") {
            Some(_) => "worldgen/biome",
            None => rest.split_once('/').map_or("", |(k, _)| k),
        };
        let ids = match kind {
            "block" => &self.blocks,
            "item" => &self.items,
            "worldgen/biome" => &self.biomes,
            _ => return true,
        };
        match id.strip_prefix('#') {
            Some(tag) => {
                let (tns, path) = tag.split_once(':').unwrap_or(("minecraft", tag));
                let rel = format!("data/{tns}/tags/{kind}/{path}.json");
                self.tag_files.contains(&rel)
                    || match kind {
                        "worldgen/biome" => self.biome_tags.contains(&format!("{tns}:{path}")),
                        _ => tns == "minecraft",
                    }
            }
            None => ids.contains(id),
        }
    }
}

/// What the mods do to one tag file, in load order (a data pack's
/// TagLoader): each adds its values, and one that replaces the tag first
/// drops vanilla's entries and the mods' before it.
#[derive(Default)]
struct TagMerge {
    /// A mod replaced the tag: vanilla's entries are left out.
    replaced: bool,
    /// The mods whose values are in `values`.
    mods: Vec<String>,
    values: Vec<J>,
}

impl TagMerge {
    /// Adds one mod's tag file. A replace that drops entries (vanilla's in a
    /// `shared` tag, one not in the mod's namespaces, or other mods') warns.
    fn add(
        &mut self,
        id: &str,
        file: &str,
        replace: bool,
        shared: bool,
        values: &[J],
    ) -> Option<String> {
        let mut warning = None;
        if replace {
            let mut dropped: Vec<String> = Vec::new();
            if shared && !self.replaced {
                dropped.push("vanilla's entries".into());
            }
            if !self.mods.is_empty() {
                dropped.push(format!("those of {}", self.mods.join(", ")));
            }
            if !dropped.is_empty() {
                warning = Some(format!(
                    "mod {id} replaces {file}: {} are left out, as a data pack's replace \
                     does; the mods after it in load order still add to it",
                    dropped.join(" and ")
                ));
            }
            self.replaced = true;
            self.mods.clear();
            self.values.clear();
        }
        if !self.mods.iter().any(|m| m == id) {
            self.mods.push(id.to_owned());
        }
        self.values.extend(values.iter().cloned());
        warning
    }
}

/// Threads rewriting the tables at once. Each holds a whole table as a tree
/// (gameplay-facts.json's is over a gigabyte with ATM10's mods), so more
/// would mostly add memory.
const TABLE_THREADS: usize = 3;

/// Rewrites the base's per-state tables for the mods' states into the
/// overlay. They are independent: [`TABLE_THREADS`] threads take them,
/// the largest first.
fn tables(base: &Path, plan: &Plan, overlay: &Overlay<'_>) -> Result<()> {
    let mut jobs: Vec<(&str, u64)> = JSON_TABLES
        .iter()
        .chain(&NBT_TABLES)
        .filter_map(|name| Some((*name, fs::metadata(base.join(name)).ok()?.len())))
        .collect();
    jobs.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    // Their directories become real here, so the threads only write files.
    for (name, _) in &jobs {
        if let Some(parent) = Path::new(name).parent() {
            overlay.dir(parent)?;
        }
    }
    let table = |name: &str| -> Result<()> {
        let path = base.join(name);
        let bytes = if JSON_TABLES.contains(&name) {
            let mut doc = read_json(&path)?;
            plan.json(name, &mut doc)
                .map_err(|e| format!("{name}: {e}"))?;
            serde_json::to_vec(&doc)?
        } else {
            let (mut named, _) = nbt::read_named(&fs::read(&path)?, Limits::tables())
                .map_err(|e| format!("{name}: {e}"))?;
            plan.nbt(&mut named.tag);
            if name == "cached-collision.nbt" || name == "worldgen/blocks/cached-collision.nbt" {
                plan.collision_shapes(&mut named.tag)?;
            }
            nbt::write_named(&named, Limits::tables()).map_err(|e| format!("{name}: {e}"))?
        };
        overlay.write(name, &bytes)
    };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(TABLE_THREADS))
            .map(|_| {
                scope.spawn(|| -> std::result::Result<(), String> {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((name, _)) = jobs.get(i) else {
                            return Ok(());
                        };
                        table(name).map_err(|e| e.to_string())?;
                    }
                })
            })
            .collect();
        workers.into_iter().try_for_each(|w| {
            w.join()
                .unwrap_or_else(|_| Err("a table thread panicked".into()))
        })
    })?;
    Ok(())
}

/// Builds the content root for `mods` over `base` in `out`. Returns the
/// warnings for the log: the shared tags a mod replaced.
pub fn build(base: &Path, mods: &[ContentMod], out: &Path) -> Result<Vec<String>> {
    // Two mods owning one namespace would mix their files.
    for (i, a) in mods.iter().enumerate() {
        for b in &mods[..i] {
            let (b, a) = (
                (b.id.as_str(), b.content.namespaces.as_slice()),
                (a.id.as_str(), a.content.namespaces.as_slice()),
            );
            if let Some(e) = horizon_mods::content::clash(b, a) {
                return Err(e.into());
            }
        }
    }
    // The overlay links to the base: a relative base would leave them dangling.
    let base = &std::path::absolute(base)?;
    let plan = Plan::new(base, mods)?;
    let overlay = Overlay::new(base, out)?;
    tables(base, &plan, &overlay)?;
    // Mod files, and every tag addition gathered per tag file.
    let mut tags: BTreeMap<String, TagMerge> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut worldgen: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    let mut additions: BTreeMap<&str, Vec<Addition>> = BTreeMap::new();
    let mut biome_adds: Vec<AddFeatures> = Vec::new();
    let mut creative: Vec<CreativeTab> = Vec::new();
    // The mods' English names, for the creative search.
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for m in mods {
        let declarations = format!("data/{}/horizon/", m.id);
        let tab_files = format!("{declarations}creative_tab/");
        let mut reader = m.content.reader()?;
        for file in &m.content.files {
            let bytes = reader.read(file)?;
            if let Some(name) = file
                .strip_prefix(&tab_files)
                .and_then(|n| n.strip_suffix(".json"))
            {
                let tab: TabFile =
                    serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
                creative.push(CreativeTab {
                    id: format!("{}:{name}", m.id),
                    file: tab,
                });
                continue;
            }
            if file.starts_with("assets/")
                && file.ends_with("/lang/en_us.json")
                && let Ok(map) = serde_json::from_slice::<BTreeMap<String, String>>(&bytes)
            {
                names.extend(map);
            }
            let is_tag = file.starts_with("data/") && file.contains("/tags/");
            if is_tag && file.ends_with(".json") {
                let doc: J = serde_json::from_slice(&bytes).map_err(|e| format!("{file}: {e}"))?;
                let Some(J::Arr(values)) = doc.get("values") else {
                    return Err(format!("{}: {file}: no values", m.id).into());
                };
                let replace = doc.get("replace") == Some(&J::Bool(true));
                let ns = file.split('/').nth(1).unwrap_or_default();
                let shared = !m.content.namespaces.iter().any(|n| n == ns);
                let merge = tags.entry(file.clone()).or_default();
                warnings.extend(merge.add(&m.id, file, replace, shared, values));
            } else if !file.starts_with(&declarations) {
                overlay.write(file, &bytes)?;
                if file.contains(BIOME_MODIFIERS) && file.ends_with(".json") {
                    let text = String::from_utf8(bytes).map_err(|e| format!("{file}: {e}"))?;
                    match AddFeatures::read(file, &text)? {
                        Some(add) => biome_adds.push(add),
                        None => warnings.push(format!(
                            "mod {}: {file}: only neoforge:add_features biome modifiers are read; left out",
                            m.id
                        )),
                    }
                } else if let Some((bundle, id)) = worldgen_entry(file) {
                    let text = String::from_utf8(bytes).map_err(|e| format!("{file}: {e}"))?;
                    let entries = worldgen.entry(bundle).or_default();
                    match Addition::read(bundle, &text).map_err(|e| format!("{file}: {e}"))? {
                        Some(addition) => additions.entry(bundle).or_default().push(addition),
                        None => entries.push((id, text)),
                    }
                } else if let Some(template) = file
                    .strip_prefix("data/")
                    .and_then(|r| r.split_once("/structure/"))
                    .filter(|(_, path)| path.ends_with(".nbt"))
                {
                    // The worldgen loader's templates: worldgen/structure/<ns>/<path>.nbt.
                    let (ns, path) = template;
                    overlay.write(&format!("worldgen/structure/{ns}/{path}"), &bytes)?;
                }
            }
        }
    }
    // A tag naming an id nobody registers fails as a whole in vanilla, and
    // carried-over mods name blocks left out or other mods' ids: such
    // entries are dropped instead, so the rest of the tag holds.
    let known = Known::new(base, &plan, &tags)?;
    for (file, merge) in &mut tags {
        merge.values.retain(|v| known.keeps(file, v));
    }
    let mut bundles: Vec<(&str, J)> = Vec::new();
    for name in TAG_BUNDLES {
        if base.join(name).exists() {
            bundles.push((name, read_json(&base.join(name))?));
        }
    }
    for (file, merge) in &tags {
        let vanilla = fs::read_to_string(base.join(file))
            .ok()
            .filter(|_| !merge.replaced);
        overlay.write(
            file,
            merge_tag(vanilla.as_deref(), &merge.values, file)?.as_bytes(),
        )?;
        // data/<ns>/tags/block/<path>.json is the block tag <ns>:<path>.
        let Some((ns, path)) = file
            .strip_prefix("data/")
            .and_then(|r| r.split_once("/tags/block/"))
            .and_then(|(ns, p)| Some((ns, p.strip_suffix(".json")?)))
        else {
            continue;
        };
        let id = format!("{ns}:{path}");
        for (name, doc) in &mut bundles {
            let text = match doc.get(&id) {
                Some(J::Str(t)) if !merge.replaced => Some(t.clone()),
                _ => None,
            };
            let merged = merge_tag(text.as_deref(), &merge.values, name)?;
            doc.set(&id, J::Str(merged));
        }
    }
    for (name, doc) in &bundles {
        overlay.write(name, &serde_json::to_vec(doc)?)?;
    }
    // Biome tags, merged into the worldgen loader's bundle as block tags are.
    let biome_tags: Vec<(String, &TagMerge)> = tags
        .iter()
        .filter_map(|(file, merge)| {
            let (ns, path) = file
                .strip_prefix("data/")?
                .split_once("/tags/worldgen/biome/")?;
            Some((format!("{ns}:{}", path.strip_suffix(".json")?), merge))
        })
        .collect();
    if !biome_tags.is_empty() && base.join(BIOME_TAGS).exists() {
        let mut doc = read_json(&base.join(BIOME_TAGS))?;
        for (id, merge) in biome_tags {
            let text = match doc.get(&id) {
                Some(J::Str(t)) if !merge.replaced => Some(t.clone()),
                _ => None,
            };
            let merged = merge_tag(text.as_deref(), &merge.values, BIOME_TAGS)?;
            doc.set(&id, J::Str(merged));
        }
        overlay.write(BIOME_TAGS, &serde_json::to_vec(&doc)?)?;
    }
    // Structures and the pools, sets, processors and features they use, then
    // what mods add to sets, pools and lists, by priority, then in mod and
    // file order.
    for list in additions.values_mut() {
        list.sort_by_key(|a| a.priority);
    }
    for (bundle, entries) in &worldgen {
        if !base.join(bundle).exists() {
            continue;
        }
        let mut doc = read_json(&base.join(bundle))?;
        for (id, text) in entries {
            doc.set(id, J::Str(text.clone()));
        }
        for addition in additions.get(bundle).into_iter().flatten() {
            addition
                .apply(&mut doc)
                .map_err(|e| format!("{bundle}: {e}"))?;
        }
        overlay.write(bundle, &serde_json::to_vec(&doc)?)?;
    }
    if !creative.is_empty() && base.join(CREATIVE).exists() {
        let mut doc = read_json(&base.join(CREATIVE))?;
        creative_tabs(&mut doc, &creative, &known, &names, &mut warnings)?;
        overlay.write(CREATIVE, &serde_json::to_vec(&doc)?)?;
    }
    // Biome modifiers, once the features and biome tags they name are in.
    if !biome_adds.is_empty() && base.join(BIOMES).exists() {
        let mut biomes = read_json(&base.join(BIOMES))?;
        let tags = read_json(&out_path(&overlay, BIOME_TAGS)).unwrap_or(J::Obj(Vec::new()));
        let placed = read_json(&out_path(&overlay, "worldgen/placed_features.json"))?;
        add_features(&mut biomes, &tags, &placed, &biome_adds)?;
        overlay.write(BIOMES, &serde_json::to_vec(&biomes)?)?;
    }
    // The commands suggest ids from `command-registries.json`: add the
    // structures, pools, sets and features the mods bring.
    let registries = out_path(&overlay, "command-registries.json");
    if let Ok(mut doc) = read_json(&registries) {
        // Registry names hold `/`, so each key is looked up by itself.
        let mut add = |keys: &[&str], new: &[J]| {
            let found = keys.iter().try_fold(&mut doc, |j, k| j.get_mut(k));
            if let Some(J::Arr(ids)) = found {
                for id in new {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
        };
        for (bundle, registry) in COMMAND_REGISTRIES {
            let new: Vec<J> = worldgen
                .get(bundle)
                .into_iter()
                .flatten()
                .map(|(id, _)| J::Str(id.clone()))
                .collect();
            add(&["by_registry", registry, "ids"], &new);
        }
        overlay.write("command-registries.json", &serde_json::to_vec(&doc)?)?;
    }
    // A block without its own blockstate looks like its template, so every
    // state still has a model.
    for m in mods {
        for block in &m.content.blocks {
            let (ns, path) = block.id.split_once(':').unwrap_or((&m.id, &block.id));
            let own = format!("assets/{ns}/blockstates/{path}.json");
            if m.content.files.contains(&own) {
                continue;
            }
            let template = block.template.trim_start_matches("minecraft:");
            let vanilla = base.join(format!("assets/minecraft/blockstates/{template}.json"));
            overlay.write(&own, &fs::read(&vanilla)?)?;
        }
    }
    let mut namespaces: Vec<&str> = mods
        .iter()
        .flat_map(|m| m.content.namespaces.iter().map(String::as_str))
        .collect();
    namespaces.sort();
    overlay.write(MARKER, &serde_json::to_vec(&namespaces)?)?;
    let remainders: BTreeMap<&str, &str> = plan
        .items
        .iter()
        .filter_map(|i| Some((i.rename.to.as_str(), i.crafting_remainder.as_deref()?)))
        .collect();
    if !remainders.is_empty() {
        overlay.write(
            horizon_items::items::MOD_CRAFT_REMAINDERS,
            &serde_json::to_vec(&remainders)?,
        )?;
    }
    // Clients must agree on the block states and the mob effects' network ids.
    if mods
        .iter()
        .any(|m| !m.content.blocks.is_empty() || !m.content.effects.is_empty())
    {
        overlay.write(crate::content_identity::MARKER, b"1")?;
    }
    overlay.write(WARNINGS, warnings.join("\n").as_bytes())?;
    Ok(warnings)
}

/// The root to load: `base` itself with no content, else the mods' overlay
/// over it, built in `cache/mod-content`.
///
/// The root is built once and kept: later starts with the same build, base
/// and mod files reuse it. With ATM10's carried mods a build takes about
/// 25 s and peaks above 3 GB, more than everything the game keeps.
pub fn resource_root(base: &Path, mods: &[ContentMod], cache: &Path) -> Result<PathBuf> {
    if mods.iter().all(|m| m.content.is_empty()) {
        return Ok(base.to_owned());
    }
    let out = cache.join("mod-content");
    let stamp = stamp(base, mods)?;
    if fs::read(out.join(STAMP)).is_ok_and(|s| s == stamp.as_bytes()) {
        // A reused root warns as the build did.
        let warnings = fs::read_to_string(out.join(WARNINGS)).unwrap_or_default();
        warnings.lines().for_each(|w| log::warn!("Horizon: {w}"));
        return Ok(out);
    }
    for warning in build(base, mods, &out)? {
        log::warn!("Horizon: {warning}");
    }
    fs::write(out.join(STAMP), stamp)?;
    Ok(out)
}

/// In a root built by [`build`]: its warnings, one per line.
const WARNINGS: &str = "horizon-mod-content-warnings.txt";

/// In a root built by [`resource_root`]: what it was built from.
const STAMP: &str = "horizon-mod-content.stamp";

/// What a content root is built from: this build of the game, the base's
/// manifest and the tables [`build`] rewrites, and every file of every mod
/// in order. Files count by size and modification time, as build tools do.
fn stamp(base: &Path, mods: &[ContentMod]) -> Result<String> {
    use sha2::{Digest, Sha256};
    let meta = |path: &Path| -> Result<String> {
        let m = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let modified = m
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(format!(
            "{} {} {}",
            path.display(),
            m.len(),
            modified.as_nanos()
        ))
    };
    let mut parts = vec!["horizon mod content 1".to_owned()];
    parts.push(meta(&std::env::current_exe()?)?);
    let base = std::path::absolute(base)?;
    parts.push(
        String::from_utf8_lossy(&fs::read(base.join("horizon-resources.json"))?).into_owned(),
    );
    for name in JSON_TABLES.iter().chain(&NBT_TABLES).chain(&TAG_BUNDLES) {
        if base.join(name).exists() {
            parts.push(meta(&base.join(name))?);
        }
    }
    for m in mods {
        parts.push(m.id.clone());
        let source = m.content.source();
        if source.is_dir() {
            for file in &m.content.files {
                // An Inox mod's generated files are in memory: their bytes stand for them.
                match m.content.generated(file) {
                    Some(bytes) => parts.push(format!("{file} {:x}", Sha256::digest(bytes))),
                    None => parts.push(meta(&source.join(file))?),
                }
            }
        } else {
            parts.push(meta(source)?);
        }
    }
    let mut hasher = Sha256::new();
    for part in &parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
#[path = "mod_content_tests.rs"]
mod tests;
