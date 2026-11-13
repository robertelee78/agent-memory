//! Bounded operation-local reuse on exact immutable inputs. Nothing is persisted.
use crate::codec::json::{self, Value};
use crate::error::Result;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub const MAX_ENTRIES: usize = 65544;
pub const MAX_MAPS: usize = 8;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Limits { pub entries: usize, pub maps: usize, pub bytes: usize }
impl Default for Limits {
    fn default() -> Self { Self { entries: MAX_ENTRIES, maps: MAX_MAPS, bytes: MAX_BYTES } }
}
#[derive(Hash, PartialEq, Eq)]
struct Identity { namespace: String, key: Vec<u8> }
struct Entry { value: Value, weight: usize, order: u64 }

pub struct Memo {
    values: HashMap<Rc<Identity>, Entry>,
    order: BTreeMap<u64, Rc<Identity>>,
    sequence: u64,
    pub bytes: usize,
    pub maps: usize,
    pub limits: Limits,
}
impl Default for Memo { fn default() -> Self { Self::new() } }
impl Memo {
    pub fn new() -> Self {
        Self { values: HashMap::new(), order: BTreeMap::new(), sequence: 0,
            bytes: 0, maps: 0, limits: Limits::default() }
    }
    pub fn len(&self) -> usize { self.values.len() }
    pub fn is_empty(&self) -> bool { self.values.is_empty() }
    fn tick(&mut self) -> u64 {
        if self.sequence == u64::MAX {
            let old = std::mem::take(&mut self.order);
            for (index, (_, identity)) in old.into_iter().enumerate() {
                self.values.get_mut(&identity).unwrap().order = index as u64;
                self.order.insert(index as u64, identity);
            }
            self.sequence = self.values.len() as u64;
        }
        let order = self.sequence; self.sequence += 1; order
    }
    fn hit(&mut self, identity: &Identity) -> Option<Value> {
        let entry = self.values.get(identity)?;
        let value = entry.value.clone(); let old_order = entry.order;
        let key = self.order.remove(&old_order).unwrap(); let order = self.tick();
        self.values.get_mut(identity).unwrap().order = order;
        self.order.insert(order, key);
        Some(value)
    }
    fn retain(&mut self, identity: Identity, value: &Value) {
        let Ok(encoded) = json::encode(value, json::COMPACT_UTF8) else { return; };
        let weight = encoded.len().saturating_add(256);
        if weight > self.limits.bytes { return; }
        let is_map = identity.namespace == "bank-format";
        // Reentrant production may already have stored this key. Assignment
        // replaces its value without moving its LRU position or double-counting it.
        if let Some(old) = self.values.get_mut(&identity) {
            self.bytes = self.bytes - old.weight + weight;
            old.value = value.clone();
            old.weight = weight;
            while !self.values.is_empty() && (self.values.len() > self.limits.entries
                || self.bytes > self.limits.bytes || self.maps > self.limits.maps)
            {
                let (_, key) = self.order.pop_first().unwrap();
                let old = self.values.remove(&key).unwrap();
                self.bytes -= old.weight;
                self.maps -= usize::from(key.namespace == "bank-format");
            }
            return;
        }
        while !self.values.is_empty() && (self.values.len() >= self.limits.entries
            || self.bytes.saturating_add(weight) > self.limits.bytes
            || (is_map && self.maps >= self.limits.maps))
        {
            let (_, key) = self.order.pop_first().unwrap();
            let old = self.values.remove(&key).unwrap(); self.bytes -= old.weight;
            self.maps -= usize::from(key.namespace == "bank-format");
        }
        let identity = Rc::new(identity); let order = self.tick();
        self.order.insert(order, identity.clone());
        self.values.insert(identity, Entry { value: value.clone(), weight, order });
        self.bytes += weight; self.maps += usize::from(is_map);
    }
    pub fn checked(&mut self, namespace: &str, key: &[u8], produce: impl FnOnce() -> Result<Value>,
        fresh_guard: Option<&mut dyn FnMut() -> Result<()>>) -> Result<Value>
    {
        let identity = Identity { namespace: namespace.into(), key: key.into() };
        if self.values.contains_key(&identity) {
            if let Some(guard) = fresh_guard { guard()?; }
            return Ok(self.hit(&identity).unwrap());
        }
        let value = produce()?; self.retain(identity, &value); Ok(value)
    }
}

pub type Handle = Rc<RefCell<Memo>>;
thread_local! { static CURRENT: RefCell<Option<Handle>> = const { RefCell::new(None) }; }
pub fn current() -> Option<Handle> { CURRENT.with(|cell| cell.borrow().clone()) }
struct Restore(Option<Handle>);
impl Drop for Restore { fn drop(&mut self) { CURRENT.with(|cell| *cell.borrow_mut() = self.0.take()); } }

/// Nested calls share one memo; unwinding restores the exact enclosing operation.
pub fn operation<T>(body: impl FnOnce(Handle) -> T) -> T {
    if let Some(existing) = current() { return body(existing); }
    let active = Rc::new(RefCell::new(Memo::new()));
    let previous = CURRENT.with(|cell| cell.replace(Some(active.clone())));
    let _restore = Restore(previous); body(active)
}
pub fn suspended<T>(body: impl FnOnce() -> T) -> T {
    let previous = CURRENT.with(|cell| cell.replace(None));
    let _restore = Restore(previous); body()
}

/// Context-aware checked call releases the memo borrow before invoking user code.
pub fn checked(namespace: &str, key: &[u8], produce: impl FnOnce() -> Result<Value>,
    fresh_guard: Option<&mut dyn FnMut() -> Result<()>>) -> Result<Value>
{
    let Some(memo) = current() else { return produce(); };
    let identity = Identity { namespace: namespace.into(), key: key.into() };
    let exists = memo.borrow().values.contains_key(&identity);
    if exists {
        if let Some(guard) = fresh_guard { guard()?; }
        if let Some(value) = memo.borrow_mut().hit(&identity) { return Ok(value); }
    }
    let value = produce()?; memo.borrow_mut().retain(identity, &value); Ok(value)
}

pub type MapInput = ([u8; 32], Value);
pub fn map_input(files: &Value) -> Option<MapInput> {
    let pairs = files.as_object()?;
    if pairs.len() > 65536 { return None; }
    if pairs.iter().any(|(name, body)| name.chars().count() > 4096
        || body.as_str().is_none_or(|body| body.chars().count() > 16384)) { return None; }
    let mut sorted: Vec<_> = pairs.iter().collect(); sorted.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut digest = Sha256::new(); let mut total = 0usize;
    for (name, body) in sorted {
        let raw = body.as_str().unwrap().as_bytes();
        total += name.len() + raw.len();
        if raw.len() > 16384 || total > MAX_BYTES { return None; }
        for part in [name.as_bytes(), raw] { digest.update((part.len() as u64).to_be_bytes()); digest.update(part); }
    }
    Some((digest.finalize().into(), files.clone()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextKey { pub digest: [u8; 32], pub type_name: &'static str, pub representation: String }
impl TextKey {
    pub fn memo_key(&self) -> Vec<u8> {
        let mut key = self.digest.to_vec(); key.extend_from_slice(self.type_name.as_bytes());
        key.push(0); key.extend_from_slice(self.representation.as_bytes()); key
    }
}
pub fn text_key(content: &Value, maximum: &Value) -> Option<TextKey> {
    let text = content.as_str()?; let maximum = maximum.as_int()?;
    if text.chars().count() > 65536 || text.len() > 65536 || !matches!(maximum.as_str(), "16384" | "65536") { return None; }
    Some(TextKey { digest: Sha256::digest(text.as_bytes()).into(), type_name: "int", representation: maximum.to_string() })
}

pub fn hex(digest: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(digest.len() * 2);
    for byte in digest { result.push(DIGITS[(byte >> 4) as usize] as char); result.push(DIGITS[(byte & 15) as usize] as char); }
    result
}
