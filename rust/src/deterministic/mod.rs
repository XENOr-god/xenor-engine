use std::collections::BTreeMap;
use std::iter::FromIterator;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterministicMap<K, V> {
    inner: BTreeMap<K, V>,
}

impl<K, V> Default for DeterministicMap<K, V> {
    fn default() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }
}

impl<K, V> DeterministicMap<K, V>
where
    K: Ord,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.inner.insert(key, value)
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner.get_mut(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.inner.values()
    }
}

impl<K, V> FromIterator<(K, V)> for DeterministicMap<K, V>
where
    K: Ord,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self {
            inner: BTreeMap::from_iter(iter),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterministicList<T> {
    inner: Vec<T>,
}

impl<T> Default for DeterministicList<T> {
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}

impl<T> DeterministicList<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn push(&mut self, value: T) {
        self.inner.push(value);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.inner.get_mut(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter()
    }

    pub fn as_slice(&self) -> &[T] {
        self.inner.as_slice()
    }
}

impl<T> IntoIterator for DeterministicList<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a DeterministicList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<T> From<Vec<T>> for DeterministicList<T> {
    fn from(value: Vec<T>) -> Self {
        Self { inner: value }
    }
}

impl<T> FromIterator<T> for DeterministicList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeterministicList, DeterministicMap};

    #[test]
    fn deterministic_map_iterates_in_key_order() {
        let mut map = DeterministicMap::new();
        map.insert(9, "z");
        map.insert(1, "a");
        map.insert(4, "d");

        let entries = map
            .iter()
            .map(|(key, value)| (*key, *value))
            .collect::<Vec<_>>();

        assert_eq!(entries, vec![(1, "a"), (4, "d"), (9, "z")]);
    }

    #[test]
    fn deterministic_list_preserves_insertion_order() {
        let mut list = DeterministicList::new();
        list.push(4);
        list.push(1);
        list.push(7);

        assert_eq!(list.iter().copied().collect::<Vec<_>>(), vec![4, 1, 7]);
    }
}
