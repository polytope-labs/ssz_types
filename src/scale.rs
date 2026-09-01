//! SCALE codec implementations, behind the `scale` feature.
//!
//! These types travel inside consensus proofs that cross a Substrate runtime boundary, so they
//! need `parity-scale-codec` alongside SSZ. This is deliberately confined to one module: it is the
//! part of this fork that upstream will never take, since it puts a Substrate dependency into an
//! SSZ library, so keeping it in a single file keeps rebases onto upstream cheap.
//!
//! # Wire compatibility
//!
//! The encodings here match `ssz-rs`, which these types replace. Every collection encodes exactly
//! as its inner `Vec<T>`, so previously encoded proofs continue to decode. Decoding re-applies the
//! type's own length rule, so a payload that would produce an over-long `FixedVector` or
//! `VariableList` is rejected at the boundary rather than surviving as an invalid value.

use crate::{FixedVector, ProgressiveList, VariableList};
use alloc::vec::Vec;
use codec::{Decode, Encode, Error, Input, Output};
use typenum::Unsigned;

impl<T: Encode, N: Unsigned> Encode for FixedVector<T, N> {
    fn encode_to<O: Output + ?Sized>(&self, dest: &mut O) {
        // Encode the slice directly: `[T]` and `Vec<T>` share a SCALE encoding, and going via the
        // slice avoids requiring `T: Clone`.
        let slice: &[T] = self;
        slice.encode_to(dest)
    }
}

impl<T: Decode, N: Unsigned> Decode for FixedVector<T, N> {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        let vec = Vec::<T>::decode(input)?;
        Self::new(vec).map_err(|_| Error::from("FixedVector: wrong number of elements"))
    }
}

impl<T: Encode, N: Unsigned> Encode for VariableList<T, N> {
    fn encode_to<O: Output + ?Sized>(&self, dest: &mut O) {
        // Encode the slice directly: `[T]` and `Vec<T>` share a SCALE encoding, and going via the
        // slice avoids requiring `T: Clone`.
        let slice: &[T] = self;
        slice.encode_to(dest)
    }
}

impl<T: Decode, N: Unsigned> Decode for VariableList<T, N> {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        let vec = Vec::<T>::decode(input)?;
        Self::new(vec).map_err(|_| Error::from("VariableList: too many elements"))
    }
}

impl<T: Encode> Encode for ProgressiveList<T> {
    fn encode_to<O: Output + ?Sized>(&self, dest: &mut O) {
        let slice: &[T] = self;
        slice.encode_to(dest)
    }
}

impl<T: Decode> Decode for ProgressiveList<T> {
    fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
        // A progressive list has no capacity, so there is no length rule to re-apply.
        Ok(Self::new(Vec::<T>::decode(input)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typenum::{U4, U8};

    #[test]
    fn collections_encode_as_their_inner_vec() {
        let values: Vec<u64> = (0..4).collect();

        // Matching `ssz-rs`, which encoded the inner `Vec` directly. Existing proofs depend on it.
        assert_eq!(
            FixedVector::<u64, U4>::new(values.clone()).unwrap().encode(),
            values.encode()
        );
        assert_eq!(
            VariableList::<u64, U8>::new(values.clone()).unwrap().encode(),
            values.encode()
        );
        assert_eq!(ProgressiveList::from(values.clone()).encode(), values.encode());
    }

    #[test]
    fn collections_round_trip() {
        let values: Vec<u64> = (0..4).collect();

        let fixed = FixedVector::<u64, U4>::new(values.clone()).unwrap();
        assert_eq!(FixedVector::<u64, U4>::decode(&mut &fixed.encode()[..]).unwrap(), fixed);

        let list = VariableList::<u64, U8>::new(values.clone()).unwrap();
        assert_eq!(VariableList::<u64, U8>::decode(&mut &list.encode()[..]).unwrap(), list);

        let progressive = ProgressiveList::from(values);
        assert_eq!(
            ProgressiveList::<u64>::decode(&mut &progressive.encode()[..]).unwrap(),
            progressive
        );
    }

    /// The length rule has to be re-applied on decode. A derive that delegated to the inner `Vec`
    /// would happily produce a `FixedVector` holding the wrong number of elements, and the SSZ
    /// root would not necessarily catch it.
    #[test]
    fn decoding_rejects_a_wrong_length_fixed_vector() {
        let too_many: Vec<u64> = (0..5).collect();
        assert!(FixedVector::<u64, U4>::decode(&mut &too_many.encode()[..]).is_err());

        let too_few: Vec<u64> = (0..3).collect();
        assert!(FixedVector::<u64, U4>::decode(&mut &too_few.encode()[..]).is_err());
    }

    #[test]
    fn decoding_rejects_an_over_long_variable_list() {
        let too_many: Vec<u64> = (0..9).collect();
        assert!(VariableList::<u64, U8>::decode(&mut &too_many.encode()[..]).is_err());

        // At the bound is fine.
        let exact: Vec<u64> = (0..8).collect();
        assert!(VariableList::<u64, U8>::decode(&mut &exact.encode()[..]).is_ok());
    }

    #[test]
    fn empty_collections_round_trip() {
        let empty: Vec<u64> = Vec::new();
        assert_eq!(
            VariableList::<u64, U8>::decode(&mut &empty.encode()[..]).unwrap().len(),
            0
        );
        assert_eq!(
            ProgressiveList::<u64>::decode(&mut &empty.encode()[..]).unwrap().len(),
            0
        );
    }
}
