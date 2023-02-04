pub(crate) const NUMBER_OF_NODES: usize = NUMBER_OF_BYZANTINE_NODES * 3 + 1;
pub(crate) const NUMBER_OF_BYZANTINE_NODES: usize = 1;
pub(crate) const NUMBER_OF_CORRECT_NODES: usize = NUMBER_OF_NODES - NUMBER_OF_BYZANTINE_NODES;
pub(crate) const QUORUM: usize = NUMBER_OF_BYZANTINE_NODES * 2 + 1;
pub(crate) const ROUND_TIMER: usize = 1000;
pub(crate) const VOTE_DELAY: usize = 2000;
pub(crate) const NUMBER_OF_TXS: usize = 1;