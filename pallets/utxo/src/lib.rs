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
pub mod issuance;
use crate::{issuance::Issuance};

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Config: frame_system::Config {
	/// Because this pallet emits events, it depends on the runtime's definition of an event.
	type Event: From<Event> + Into<<Self as frame_system::Config>::Event>;
	type Issuance: Issuance<<Self as frame_system::Config>::BlockNumber, Value>;
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

		// the total reward that will be distributed to the miner when processing each block
		pub RewardTotal get(fn reward_total): Value;
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
		RewardsIssued(Value, H256),
		RewardsWasted,
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

			let mut reward = 0;
			Self::update_storage(&tx, reward)?;

			// 3. emit success event
			Self::deposit_event(Event::TransactionSuccess(tx));

			Ok(())
		}

		// function executed at the end of each block
		fn on_finalize() {
			match T::BlockAuthor::block_author() {
				// Block author did not provide key to claim reward
				None => Self::deposit_event(Event::RewardsWasted),
				// Block author did provide key, so issue thir reward
				Some(author) => Self::disperse_reward(&author),
			}
		}
	}
}

// Add additional helper function that can be accessible in anywhere we import Config
impl<T: Config> Module<T> {
	fn update_storage(tx: &Transaction, reward: Value) -> DispatchResult {
		let new_total = RewardTotal::get()
			.checked_add(reward)
			.ok_or("reward overflow")?;

		RewardTotal::put(new_total);

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

	fn disperse_reward(author: &Public) {
		// 1. divide the reward fairly amongst all validators processing the block
		let reward = RewardTotal::take() + T::Issuance::issuance(<frame_system::Module<T>>::block_number());

		// 2. create utxo for validator
		let utxo = TransactionOutput{
			value: reward,
			pubkey: H256::from_slice(author.as_slice()),
		};
		// <frame_system::Module<T>>::block_number();
		let current_block = <frame_system::Module<T>>::block_number().saturated_into::<u64>();
		let hash = BlakeTwo256::hash_of(&(&utxo, current_block));

		// Store the Utxo
		UtxoStore::insert(hash, utxo);

		Self::deposit_event(Event::RewardsIssued(reward, hash));
	}
}
