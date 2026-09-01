use alloc::vec::Vec;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::slice::SliceIndex;
use serde_derive::{Deserialize, Serialize};
use tree_hash::{Hash256, ProgressiveMerkleHasher, TreeHash, TreeHashType};

/// An SSZ `ProgressiveList`, as defined by [EIP-7916].
///
/// Serialization is identical to a `List`, so this is a drop in replacement on the wire. Only
/// merkleization differs: the chunks are hashed into the progressive spine rather than a balanced
/// tree padded out to a capacity, which is what lets the type carry no capacity bound at all.
///
/// The absence of a bound is the point. Gloas moved twelve `BeaconState` fields to this type
/// precisely so that their merkleization stops depending on a limit that has to be revised, and
/// so that a given element keeps its generalized index as the list grows.
///
/// [EIP-7916]: https://eips.ethereum.org/EIPS/eip-7916
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProgressiveList<T> {
    vec: Vec<T>,
}

impl<T> Default for ProgressiveList<T> {
    fn default() -> Self {
        Self { vec: Vec::new() }
    }
}

impl<T> ProgressiveList<T> {
    /// An empty list.
    pub fn empty() -> Self {
        Self { vec: Vec::new() }
    }

    /// Wrap `vec`. Unlike `VariableList` this cannot fail, because there is no capacity to exceed.
    pub fn new(vec: Vec<T>) -> Self {
        Self { vec }
    }

    /// The number of elements.
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// True if there are no elements.
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Append an element.
    pub fn push(&mut self, value: T) {
        self.vec.push(value);
    }

    /// The underlying vector.
    pub fn into_vec(self) -> Vec<T> {
        self.vec
    }
}

impl<T> From<Vec<T>> for ProgressiveList<T> {
    fn from(vec: Vec<T>) -> Self {
        Self { vec }
    }
}

impl<T> From<ProgressiveList<T>> for Vec<T> {
    fn from(list: ProgressiveList<T>) -> Vec<T> {
        list.vec
    }
}

impl<T> FromIterator<T> for ProgressiveList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            vec: iter.into_iter().collect(),
        }
    }
}

impl<T> IntoIterator for ProgressiveList<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.vec.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a ProgressiveList<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.vec.iter()
    }
}

impl<T> Deref for ProgressiveList<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.vec
    }
}

impl<T> DerefMut for ProgressiveList<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.vec
    }
}

impl<T> AsRef<[T]> for ProgressiveList<T> {
    fn as_ref(&self) -> &[T] {
        &self.vec
    }
}

impl<T, I: SliceIndex<[T]>> Index<I> for ProgressiveList<T> {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        Index::index(&self.vec, index)
    }
}

impl<T, I: SliceIndex<[T]>> IndexMut<I> for ProgressiveList<T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(&mut self.vec, index)
    }
}

/// Merkleize `vec` into the progressive spine.
///
/// This mirrors [`crate::tree_hash::vec_tree_hash_root`], but swaps the balanced `MerkleHasher`
/// for [`ProgressiveMerkleHasher`] and so needs no capacity argument.
pub fn progressive_vec_tree_hash_root<T>(vec: &[T]) -> Hash256
where
    T: TreeHash,
{
    let mut hasher = ProgressiveMerkleHasher::new();

    match T::tree_hash_type() {
        TreeHashType::Basic => {
            for item in vec {
                hasher
                    .write(&item.tree_hash_packed_encoding())
                    .expect("progressive hasher imposes no leaf limit");
            }
        }
        TreeHashType::Container | TreeHashType::List | TreeHashType::Vector => {
            for item in vec {
                hasher
                    .write(item.tree_hash_root().as_slice())
                    .expect("progressive hasher imposes no leaf limit");
            }
        }
    }

    hasher
        .finish()
        .expect("progressive hasher should not have a remaining buffer")
}

impl<T> TreeHash for ProgressiveList<T>
where
    T: TreeHash,
{
    fn tree_hash_type() -> TreeHashType {
        TreeHashType::List
    }

    fn tree_hash_packed_encoding(&self) -> tree_hash::PackedEncoding {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_packing_factor() -> usize {
        unreachable!("List should never be packed.")
    }

    fn tree_hash_root(&self) -> Hash256 {
        let root = progressive_vec_tree_hash_root::<T>(&self.vec);
        tree_hash::mix_in_length(&root, self.len())
    }
}

impl<T> ssz::Encode for ProgressiveList<T>
where
    T: ssz::Encode,
{
    fn is_ssz_fixed_len() -> bool {
        <Vec<T>>::is_ssz_fixed_len()
    }

    fn ssz_fixed_len() -> usize {
        <Vec<T>>::ssz_fixed_len()
    }

    fn ssz_bytes_len(&self) -> usize {
        self.vec.ssz_bytes_len()
    }

    fn ssz_append(&self, buf: &mut Vec<u8>) {
        self.vec.ssz_append(buf)
    }
}

impl<T> ssz::Decode for ProgressiveList<T>
where
    T: ssz::Decode,
{
    fn is_ssz_fixed_len() -> bool {
        false
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        if bytes.is_empty() {
            return Ok(Self::default());
        }
        // A progressive list has no capacity, so unlike `VariableList` there is nothing to check
        // the decoded length against.
        <Vec<T>>::from_ssz_bytes(bytes).map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_hash::mix_in_length;

    #[test]
    fn serialization_matches_a_plain_vec() {
        use ssz::{Decode, Encode};

        let values: Vec<u64> = (0..20).collect();
        let list = ProgressiveList::from(values.clone());

        // EIP-7916 changes merkleization only, so the wire format must be byte identical.
        assert_eq!(list.as_ssz_bytes(), values.as_ssz_bytes());

        let decoded = ProgressiveList::<u64>::from_ssz_bytes(&values.as_ssz_bytes()).unwrap();
        assert_eq!(decoded.into_vec(), values);
    }

    #[test]
    fn empty_list_round_trips() {
        use ssz::{Decode, Encode};

        let list = ProgressiveList::<u64>::empty();
        assert_eq!(list.as_ssz_bytes(), Vec::<u8>::new());
        assert!(ProgressiveList::<u64>::from_ssz_bytes(&[]).unwrap().is_empty());
    }

    #[test]
    fn root_mixes_in_the_length() {
        let list = ProgressiveList::from((0..5u64).collect::<Vec<_>>());
        let expected = mix_in_length(&progressive_vec_tree_hash_root(&list.vec), 5);
        assert_eq!(list.tree_hash_root(), expected);
    }

    /// Two lists of different length must not share a root even when one is a prefix of the other,
    /// which is what mixing in the length buys.
    #[test]
    fn length_is_bound_into_the_root() {
        let short = ProgressiveList::from((0..4u64).collect::<Vec<_>>());
        let long = ProgressiveList::from((0..5u64).collect::<Vec<_>>());
        assert_ne!(short.tree_hash_root(), long.tree_hash_root());
    }

    /// Unlike `VariableList`, growing the list does not move existing elements in the tree. This is
    /// the property EIP-7916 exists for, so it is worth pinning.
    #[test]
    fn growing_the_list_is_stable_for_earlier_chunks() {
        let mut hasher_small = ProgressiveMerkleHasher::new();
        let mut hasher_large = ProgressiveMerkleHasher::new();

        let chunks: Vec<Hash256> = (0..10u64).map(|i| Hash256::from(i.tree_hash_root())).collect();

        for chunk in chunks.iter().take(3) {
            hasher_small.write(chunk.as_slice()).unwrap();
            hasher_large.write(chunk.as_slice()).unwrap();
        }
        let small = hasher_small.finish().unwrap();

        for chunk in chunks.iter().skip(3) {
            hasher_large.write(chunk.as_slice()).unwrap();
        }
        let large = hasher_large.finish().unwrap();

        // The roots differ, but only because the spine gained levels, not because the first three
        // chunks moved. Their proofs are checked in `tree_hash::proof`.
        assert_ne!(small, large);
    }

    #[test]
    fn composite_elements_hash_by_root() {
        // `Hash256` is a vector type, so it takes the non basic branch of the hasher.
        let list = ProgressiveList::from(alloc::vec![Hash256::repeat_byte(1), Hash256::repeat_byte(2)]);
        let expected = mix_in_length(&progressive_vec_tree_hash_root(&list.vec), 2);
        assert_eq!(list.tree_hash_root(), expected);
    }
}
