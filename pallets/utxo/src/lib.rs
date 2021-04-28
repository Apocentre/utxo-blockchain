#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use frame_support::{
	decl_event, decl_error, decl_module, decl_storage,
	dispatch::{DispatchResult, Vec},
};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::{
	crypto::Public as _,
	H256,
	H512,
	sr25519::{Public, Signature},
};
use sp_std::collections::btree_map::BTreeMap;
use sp_runtime::{
	traits::{BlakeTwo256, Hash, SaturatedConversion},
	transaction_validity::{TransactionLongevity, ValidTransaction},
};
// use super::{block_author::BlockAuthor, issuance::Issuance};

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Config: frame_system::Config {
	/// Because this pallet emits events, it depends on the runtime's definition of an event.
	type Event: From<Event> + Into<<Self as frame_system::Config>::Event>;
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Default, Clone, Encode, Decode, Hash, Debug)]
pub struct TransactionInput {
	// reference to a future UTXO to be spent
	pub outpoint: H256,

	// proof that the tx owner is authorised to spent the referred UTXO
	pub sigscript: H512,
}

pub type Value = u128;

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Default, Clone, Encode, Decode, Hash, Debug)]
pub struct TransactionOutput {
	// size of the UTXO
	pub value: Value,

	// the key of the onwer of the transaction output
	pub pubkey: H256,
}

#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, PartialOrd, Ord, Default, Clone, Encode, Decode, Hash, Debug)]
pub struct Transaction {
	pub inputs: Vec<TransactionInput>,
	pub outputs: Vec<TransactionOutput>,
}

// The pallet's runtime storage items.
// https://substrate.dev/docs/en/knowledgebase/runtime/storage
decl_storage! {
	trait Store for Module<T: Config> as UtxoModule {
		// seed data from genesis
		UtxoStore build(|config: &GenesisConfig| {
			config.genesis_utxos
				.iter()
				.cloned()
				.map(|u| (BlakeTwo256::hash_of(&u), u))
				.collect::<Vec<_>>()
		}): map hasher(identity) H256 => Option<TransactionOutput>;
	}

	add_extra_genesis {
		// create a config property that will be pre-populated from the genesis file
		config(genesis_utxos): Vec<TransactionOutput>;
	}
}

// Pallets use events to inform users when important changes are made.
// Event documentation should end with an array that provides descriptive names for parameters.
// https://substrate.dev/docs/en/knowledgebase/runtime/events
decl_event! {
	pub enum Event {
		TransactionSuccess(Transaction),
	}
}

// Errors inform users that something went wrong.
decl_error! {
	pub enum Error for Module<T: Config> {

	}
}

// Dispatchable functions allows users to interact with the pallet and invoke state changes.
// These functions materialize as "extrinsics", which are often compared to transactions.
// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		fn deposit_event() = default;

		#[weight = 10_000]
		pub fn spend(_origin, tx: Transaction) -> DispatchResult {
			// 1. check that the transaction is valid

			// 2. update the storage
			Self::update_storage(&tx)?;

			// 3. emit success event
			Self::deposit_event(Event::TransactionSuccess(tx));

			Ok(())
		}
	}
}

// Add additional helper function that can be accessible in anywhere we import Config
impl<T: Config> Module<T> {
	fn update_storage(tx: &Transaction) -> DispatchResult {
		// 1. Remove all input utxos from the UtxoStore
		for input in &tx.inputs {
			UtxoStore::remove(input.outpoint);
		}

		// 2. Create a new utxo
		let mut index: u64 = 0;
		for output in &tx.outputs {
			// Make sure the key is unique by using the entire tx and a unique index
			let key = BlakeTwo256::hash_of(&(&tx.encode(), index));
			index = index.checked_add(1).ok_or("output index overflow")?;
			UtxoStore::insert(key, output);
		}
		Ok(())
	}
}
