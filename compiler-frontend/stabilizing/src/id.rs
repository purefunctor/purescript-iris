use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::{any, fmt, hash, mem};

use syntax::ast::{AstNode, AstPtr};

use crate::StabilizedModule;

pub struct AstId<N: AstNode> {
    pub(crate) id: NonZeroU32,
    phantom: PhantomData<fn() -> AstPtr<N>>,
}

impl<N: AstNode> AstId<N> {
    pub const fn new(id: NonZeroU32) -> AstId<N> {
        AstId { id, phantom: PhantomData }
    }

    pub const fn into_raw(self) -> NonZeroU32 {
        self.id
    }
}

impl<N: AstNode> fmt::Debug for AstId<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("AstId<{}>({})", any::type_name::<N>(), self.id))
    }
}

impl<N: AstNode> Clone for AstId<N> {
    fn clone(&self) -> AstId<N> {
        *self
    }
}

impl<N: AstNode> Copy for AstId<N> {}

impl<N: AstNode> PartialEq for AstId<N> {
    fn eq(&self, other: &AstId<N>) -> bool {
        self.id == other.id
    }
}

impl<N: AstNode> Eq for AstId<N> {}

impl<N: AstNode> PartialOrd for AstId<N> {
    fn partial_cmp(&self, other: &AstId<N>) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<N: AstNode> Ord for AstId<N> {
    fn cmp(&self, other: &AstId<N>) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<N: AstNode> hash::Hash for AstId<N> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub struct AstIdMap<I, V> {
    positions: Vec<Option<NonZeroU32>>,
    entries: Vec<(I, V)>,
}

impl<N: AstNode, V> AstIdMap<AstId<N>, V> {
    pub fn new(stabilized: &StabilizedModule) -> AstIdMap<AstId<N>, V> {
        let positions = vec![None; stabilized.len()];
        AstIdMap { positions, entries: Vec::new() }
    }

    pub fn insert(&mut self, id: AstId<N>, value: V) -> Option<V> {
        let index = id.id.get() as usize - 1;
        if index >= self.positions.len() {
            self.positions.resize(index + 1, None);
        }
        if let Some(position) = self.positions[index] {
            let (_, previous) = &mut self.entries[position.get() as usize - 1];
            return Some(mem::replace(previous, value));
        }
        self.entries.push((id, value));
        self.positions[index] = NonZeroU32::new(self.entries.len() as u32);
        None
    }

    pub fn get(&self, id: AstId<N>) -> Option<&V> {
        let index = id.id.get() as usize - 1;
        let position = (*self.positions.get(index)?)?;
        let (_, value) = &self.entries[position.get() as usize - 1];
        Some(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (AstId<N>, &V)> {
        self.entries.iter().map(|(id, value)| (*id, value))
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, value)| value)
    }
}

impl<I, V> Default for AstIdMap<I, V> {
    fn default() -> AstIdMap<I, V> {
        AstIdMap { positions: Vec::new(), entries: Vec::new() }
    }
}

impl<N: AstNode, V: fmt::Debug> fmt::Debug for AstIdMap<AstId<N>, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<N: AstNode, V: PartialEq> PartialEq for AstIdMap<AstId<N>, V> {
    fn eq(&self, other: &AstIdMap<AstId<N>, V>) -> bool {
        self.entries.len() == other.entries.len()
            && self.iter().all(|(id, value)| other.get(id) == Some(value))
    }
}

impl<N: AstNode, V: Eq> Eq for AstIdMap<AstId<N>, V> {}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use syntax::cst;

    use super::{AstId, AstIdMap};

    fn id(raw: u32) -> AstId<cst::Annotation> {
        AstId::new(NonZeroU32::new(raw).unwrap())
    }

    #[test]
    fn test_insert_and_get() {
        let mut map = AstIdMap::default();
        assert_eq!(map.insert(id(5), "five"), None);
        assert_eq!(map.insert(id(2), "two"), None);
        assert_eq!(map.insert(id(5), "FIVE"), Some("five"));

        assert_eq!(map.get(id(2)), Some(&"two"));
        assert_eq!(map.get(id(5)), Some(&"FIVE"));
        assert_eq!(map.get(id(3)), None);
        assert_eq!(map.get(id(9)), None);
        assert_eq!(map.iter().count(), 2);
    }

    #[test]
    fn test_equality() {
        let mut map_a = AstIdMap::default();
        map_a.insert(id(1), 'a');
        map_a.insert(id(4), 'b');

        let mut map_b = AstIdMap::default();
        map_b.insert(id(4), 'b');
        map_b.insert(id(1), 'a');

        assert!(map_a == map_b);

        map_b.insert(id(4), 'c');
        assert!(map_a != map_b);

        let mut map_c = AstIdMap::default();
        map_c.insert(id(1), 'a');
        map_c.insert(id(3), 'b');
        assert!(map_a != map_c);

        map_c.insert(id(4), 'b');
        assert!(map_a != map_c);
    }
}
