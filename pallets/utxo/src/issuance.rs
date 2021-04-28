#![cfg_attr(not(feature = "std"), no_std)]

pub trait Issuance<BlockNumber, Balance> {
	fn issuance(block: BlockNumber) -> Balance;
}

// Minimal implementations for when you don't actually want any issuance
impl Issuance<u32, u128> for () {
	fn issuance(_block: u32) -> u128 {
		0
	}
}

impl Issuance<u64, u128> for () {
	fn issuance(_block: u64) -> u128 { 0 }
}

/// A type that will follow the issuance model from Bitcoin
/// Initial issuance is 50 / block
/// Issuance is cut in half every 210,000 blocks
/// cribbed from github.com/Bitcoin-ABC/bitcoin-abc/blob/9c7b12e6f128a59423f4de3d6d4b5231ebe9aac2/src/validation.cpp#L1
pub struct HalvingIssuance;

const HALVING_EVERY_BLOCKS: u32 = 210_000;
const INITIAL_ISSUANCE: u32  = 50;

impl Issuance for HalvingIssuance {
	fn issuance(block: BlockNumber) -> Balance {
		let halvings = block / HALVING_EVERY_BLOCKS;

		// Force block reward to zero when right shift is undefined.
		if halvings >= 64 {
			return 0;
		}

		// Subsidy is cut in half every 210,000 blocks which will occur
		// approximately every 4 years.
		(INITIAL_ISSUANCE >> halvings).into()
	}
}
