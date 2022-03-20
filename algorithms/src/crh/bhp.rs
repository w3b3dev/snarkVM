// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use crate::{hash_to_curve::hash_to_curve, CRHError, CRH};
use snarkvm_curves::{AffineCurve, ProjectiveCurve};
use snarkvm_fields::{ConstraintFieldError, Field, PrimeField, ToConstraintField};
use snarkvm_utilities::BigInteger;

use std::{borrow::Borrow, fmt::Debug, sync::Arc};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

// The stack is currently allocated with the following size
// because we cannot specify them using the trait consts.
const MAX_WINDOW_SIZE: usize = 64;
const MAX_NUM_WINDOWS: usize = 4096;

pub const BOWE_HOPWOOD_CHUNK_SIZE: usize = 3;
pub const BOWE_HOPWOOD_LOOKUP_SIZE: usize = 2usize.pow(BOWE_HOPWOOD_CHUNK_SIZE as u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BHPCRH<G: ProjectiveCurve, const NUM_WINDOWS: usize, const WINDOW_SIZE: usize> {
    pub bases: Arc<Vec<Vec<G>>>,
    base_lookup: Vec<Vec<[G; BOWE_HOPWOOD_LOOKUP_SIZE]>>,
}

impl<G: ProjectiveCurve, const NUM_WINDOWS: usize, const WINDOW_SIZE: usize> CRH
    for BHPCRH<G, NUM_WINDOWS, WINDOW_SIZE>
{
    type Output = <G::Affine as AffineCurve>::BaseField;
    type Parameters = Arc<Vec<Vec<G>>>;

    fn setup(message: &str) -> Self {
        fn calculate_maximum_window_size<F: PrimeField>() -> usize {
            let upper_limit = F::modulus_minus_one_div_two();
            let mut c = 0;
            let mut range = F::BigInteger::from(2_u64);
            while range < upper_limit {
                range.muln(4);
                c += 1;
            }
            c
        }

        let maximum_window_size = calculate_maximum_window_size::<G::ScalarField>();
        if WINDOW_SIZE > maximum_window_size {
            panic!(
                "BHP CRH must have a window size resulting in scalars < (p-1)/2, \
                 maximum segment size is {}",
                maximum_window_size
            );
        }

        let bases = (0..NUM_WINDOWS)
            .map(|index| {
                // Construct an indexed message to attempt to sample a base.
                let (generator, _, _) = hash_to_curve::<G::Affine>(&format!("{message} at {index}"));
                let mut base = generator.into_projective();
                // Compute the generators for the sampled base.
                let mut powers = Vec::with_capacity(WINDOW_SIZE);
                for _ in 0..WINDOW_SIZE {
                    powers.push(base);
                    for _ in 0..4 {
                        base.double_in_place();
                    }
                }
                powers
            })
            .collect::<Vec<Vec<G>>>();

        let base_lookup = crate::cfg_iter!(bases)
            .map(|x| {
                x.iter()
                    .map(|g| {
                        let mut out = [G::zero(); BOWE_HOPWOOD_LOOKUP_SIZE];
                        for (i, element) in out.iter_mut().enumerate().take(BOWE_HOPWOOD_LOOKUP_SIZE) {
                            let mut encoded = *g;
                            if (i & 0x01) != 0 {
                                encoded += g;
                            }
                            if (i & 0x02) != 0 {
                                encoded += g.double();
                            }
                            if (i & 0x04) != 0 {
                                encoded = encoded.neg();
                            }
                            *element = encoded;
                        }
                        out
                    })
                    .collect()
            })
            .collect::<Vec<Vec<[G; BOWE_HOPWOOD_LOOKUP_SIZE]>>>();
        debug_assert_eq!(base_lookup.len(), NUM_WINDOWS);
        base_lookup.iter().for_each(|bases| debug_assert_eq!(bases.len(), WINDOW_SIZE));

        Self { bases: Arc::new(bases), base_lookup }
    }

    fn hash(&self, input: &[bool]) -> Result<Self::Output, CRHError> {
        Ok(self.hash_bits_inner(input)?.into_affine().to_x_coordinate())
    }

    fn parameters(&self) -> &Self::Parameters {
        &self.bases
    }
}

impl<G: ProjectiveCurve, const NUM_WINDOWS: usize, const WINDOW_SIZE: usize> BHPCRH<G, NUM_WINDOWS, WINDOW_SIZE> {
    /// Precondition: number of elements in `input` == `num_bits`.
    pub(crate) fn hash_bits_inner(&self, input: &[bool]) -> Result<G, CRHError> {
        // Input-independent sanity checks.
        debug_assert!(WINDOW_SIZE <= MAX_WINDOW_SIZE);
        debug_assert!(NUM_WINDOWS <= MAX_NUM_WINDOWS);
        debug_assert_eq!(self.bases.len(), NUM_WINDOWS, "Incorrect number of windows ({:?}) for BHP", self.bases.len(),);
        self.bases.iter().for_each(|bases| debug_assert_eq!(bases.len(), WINDOW_SIZE));
        debug_assert_eq!(BOWE_HOPWOOD_CHUNK_SIZE, 3);

        if input.len() > WINDOW_SIZE * NUM_WINDOWS {
            return Err(CRHError::IncorrectInputLength(input.len(), WINDOW_SIZE, NUM_WINDOWS));
        }

        // overzealous but stack allocation
        let mut input_stack = [false; MAX_WINDOW_SIZE * MAX_NUM_WINDOWS + BOWE_HOPWOOD_CHUNK_SIZE + 1];
        input_stack[..input.len()].iter_mut().zip(input).for_each(|(b, i)| *b = *i.borrow());

        let mut bit_len = input.len();
        if bit_len % BOWE_HOPWOOD_CHUNK_SIZE != 0 {
            bit_len += BOWE_HOPWOOD_CHUNK_SIZE - (bit_len % BOWE_HOPWOOD_CHUNK_SIZE);
        }
        debug_assert_eq!(bit_len % BOWE_HOPWOOD_CHUNK_SIZE, 0);

        // Compute sum of h_i^{sum of
        // (1-2*c_{i,j,2})*(1+c_{i,j,0}+2*c_{i,j,1})*2^{4*(j-1)} for all j in segment}
        // for all i. Described in section 5.4.1.7 in the Zcash protocol
        // specification.
        Ok(input_stack[..bit_len]
            .chunks(WINDOW_SIZE * BOWE_HOPWOOD_CHUNK_SIZE)
            .zip(&self.base_lookup)
            .flat_map(|(bits, bases)| {
                bits.chunks(BOWE_HOPWOOD_CHUNK_SIZE).zip(bases).map(|(chunk_bits, base)| {
                    base[(chunk_bits[0] as usize) | (chunk_bits[1] as usize) << 1 | (chunk_bits[2] as usize) << 2]
                })
            })
            .sum())
    }
}

impl<F: Field, G: ProjectiveCurve + ToConstraintField<F>, const NUM_WINDOWS: usize, const WINDOW_SIZE: usize>
    ToConstraintField<F> for BHPCRH<G, NUM_WINDOWS, WINDOW_SIZE>
{
    #[inline]
    fn to_field_elements(&self) -> Result<Vec<F>, ConstraintFieldError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_curves::edwards_bls12::EdwardsProjective;

    const NUM_WINDOWS: usize = 8;
    const WINDOW_SIZE: usize = 32;

    #[test]
    fn test_bhp_sanity_check() {
        let crh = <BHPCRH<EdwardsProjective, NUM_WINDOWS, WINDOW_SIZE> as CRH>::setup("test_bowe_pedersen");
        let input = vec![127u8; 32];

        let output = crh.hash_bytes(&input).unwrap();
        assert_eq!(
            &*output.to_string(),
            "2591648422993904809826711498838675948697848925001720514073745852367402669969"
        );
    }
}
