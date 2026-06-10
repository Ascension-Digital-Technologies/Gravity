use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfStats {
    pub pool_capacity: usize,
    pub pool_available: usize,
    pub pool_checked_out: usize,
    pub interned_symbols: usize,
    pub interned_accounts: usize,
    pub reusable_vec_capacity: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Interned(pub Arc<str>);

impl Interned {
    pub fn as_str(&self) -> &str { &self.0 }
}

#[derive(Clone, Debug, Default)]
pub struct Interner {
    map: HashMap<String, Arc<str>>,
}

impl Interner {
    pub fn with_capacity(capacity: usize) -> Self { Self { map: HashMap::with_capacity(capacity) } }

    pub fn intern(&mut self, value: impl AsRef<str>) -> Interned {
        let key = value.as_ref();
        if let Some(existing) = self.map.get(key) { return Interned(existing.clone()); }
        let owned = key.to_owned();
        let arc: Arc<str> = Arc::from(owned.as_str());
        self.map.insert(owned, arc.clone());
        Interned(arc)
    }

    pub fn len(&self) -> usize { self.map.len() }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }
}

#[derive(Debug)]
pub struct SlotPool<T> {
    free: Vec<T>,
    capacity: usize,
    checked_out: usize,
}

impl<T> SlotPool<T> {
    pub fn new_with(capacity: usize, mut make: impl FnMut(usize) -> T) -> Self {
        let mut free = Vec::with_capacity(capacity);
        for i in 0..capacity { free.push(make(i)); }
        Self { free, capacity, checked_out: 0 }
    }

    pub fn checkout(&mut self) -> Option<T> {
        let value = self.free.pop()?;
        self.checked_out = self.checked_out.saturating_add(1);
        Some(value)
    }

    pub fn release(&mut self, value: T) {
        if self.free.len() < self.capacity {
            self.free.push(value);
            self.checked_out = self.checked_out.saturating_sub(1);
        }
    }

    pub fn available(&self) -> usize { self.free.len() }
    pub fn checked_out(&self) -> usize { self.checked_out }
    pub fn capacity(&self) -> usize { self.capacity }
}

#[derive(Debug)]
pub struct ReusableVec<T> {
    inner: Vec<T>,
}

impl<T> ReusableVec<T> {
    pub fn with_capacity(capacity: usize) -> Self { Self { inner: Vec::with_capacity(capacity) } }
    pub fn clear(&mut self) { self.inner.clear(); }
    pub fn push(&mut self, value: T) { self.inner.push(value); }
    pub fn len(&self) -> usize { self.inner.len() }
    pub fn capacity(&self) -> usize { self.inner.capacity() }
    pub fn is_empty(&self) -> bool { self.inner.is_empty() }
    pub fn as_slice(&self) -> &[T] { &self.inner }
}

#[derive(Clone, Debug, Default)]
pub struct FastCounterMap<K> {
    map: HashMap<K, i128>,
}

impl<K: Eq + Hash> FastCounterMap<K> {
    pub fn with_capacity(capacity: usize) -> Self { Self { map: HashMap::with_capacity(capacity) } }
    pub fn add(&mut self, key: K, value: i128) { *self.map.entry(key).or_insert(0) += value; }
    pub fn clear(&mut self) { self.map.clear(); }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn is_empty(&self) -> bool { self.map.is_empty() }
    pub fn total_abs(&self) -> i128 { self.map.values().map(|v| v.abs()).sum() }
}

#[derive(Debug)]
pub struct PerfArena {
    pub symbols: Interner,
    pub accounts: Interner,
    pub reusable: ReusableVec<u64>,
    pub pool: SlotPool<u64>,
}

impl PerfArena {
    pub fn new(symbol_capacity: usize, account_capacity: usize, vec_capacity: usize, pool_capacity: usize) -> Self {
        Self {
            symbols: Interner::with_capacity(symbol_capacity),
            accounts: Interner::with_capacity(account_capacity),
            reusable: ReusableVec::with_capacity(vec_capacity),
            pool: SlotPool::new_with(pool_capacity, |i| i as u64),
        }
    }

    pub fn record(&mut self, symbol: &str, account: &str, sequence: u64) {
        let sym = self.symbols.intern(symbol);
        let acct = self.accounts.intern(account);
        std::hint::black_box(sym.as_str());
        std::hint::black_box(acct.as_str());
        if let Some(slot) = self.pool.checkout() {
            self.reusable.push(sequence ^ slot);
            self.pool.release(slot);
        }
        if self.reusable.len() >= self.reusable.capacity() { self.reusable.clear(); }
    }

    pub fn stats(&self) -> PerfStats {
        PerfStats {
            pool_capacity: self.pool.capacity(),
            pool_available: self.pool.available(),
            pool_checked_out: self.pool.checked_out(),
            interned_symbols: self.symbols.len(),
            interned_accounts: self.accounts.len(),
            reusable_vec_capacity: self.reusable.capacity(),
        }
    }
}
