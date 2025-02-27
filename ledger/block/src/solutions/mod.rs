// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod bytes;
mod merkle;
mod serialize;
mod string;

use console::{network::prelude::*, types::Field};
use ledger_coinbase::{CoinbaseSolution, PuzzleCommitment};
use ledger_committee::Committee;
use ledger_narwhal_batch_header::BatchHeader;

#[derive(Clone, Eq, PartialEq)]
pub struct Solutions<N: Network> {
    /// The prover solutions for the coinbase puzzle.
    solutions: Option<CoinbaseSolution<N>>,
}

impl<N: Network> Solutions<N> {
    /// The maximum number of aborted solutions allowed in a block.
    pub const MAX_ABORTED_SOLUTIONS: usize = BatchHeader::<N>::MAX_TRANSMISSIONS_PER_BATCH
        * BatchHeader::<N>::MAX_GC_ROUNDS
        * Committee::<N>::MAX_COMMITTEE_SIZE as usize;
}

impl<N: Network> From<Option<CoinbaseSolution<N>>> for Solutions<N> {
    /// Initializes a new instance of the solutions.
    fn from(solutions: Option<CoinbaseSolution<N>>) -> Self {
        // Return the solutions.
        Self { solutions }
    }
}

impl<N: Network> Solutions<N> {
    /// Initializes a new instance of the solutions.
    pub fn new(solutions: CoinbaseSolution<N>) -> Result<Self> {
        // Return the solutions.
        Ok(Self { solutions: Some(solutions) })
    }

    /// Returns `true` if the solutions are empty.
    pub fn is_empty(&self) -> bool {
        self.solutions.is_none()
    }

    /// Returns the number of solutions.
    pub fn len(&self) -> usize {
        match &self.solutions {
            Some(solutions) => solutions.len(),
            None => 0,
        }
    }
}

impl<N: Network> Solutions<N> {
    /// Returns an iterator over the solution IDs.
    pub fn solution_ids<'a>(&'a self) -> Box<dyn Iterator<Item = &'a PuzzleCommitment<N>> + 'a> {
        match &self.solutions {
            Some(solutions) => Box::new(solutions.keys()),
            None => Box::new(std::iter::empty::<&PuzzleCommitment<N>>()),
        }
    }
}

impl<N: Network> Solutions<N> {
    /// Returns the combined sum of the prover solutions.
    pub fn to_combined_proof_target(&self) -> Result<u128> {
        match &self.solutions {
            Some(solutions) => solutions.to_combined_proof_target(),
            None => Ok(0),
        }
    }
}

impl<N: Network> Deref for Solutions<N> {
    type Target = Option<CoinbaseSolution<N>>;

    /// Returns the solutions.
    fn deref(&self) -> &Self::Target {
        &self.solutions
    }
}
