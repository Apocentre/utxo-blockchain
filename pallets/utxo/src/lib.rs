#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, Encode};
use frame_support::{
	decl_event, decl_error, decl_module, decl_storage, ensure,
	dispatch::{DispatchResult, Vec},
	traits::{FindAuthor},
};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::{
	crypto::Public as _,
	H256,
	H512,
	sr25519::{Public, Signature},
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_std::collections::btree_map::BTreeMap;
use sp_runtime::{
	traits::{BlakeTwo256, Hash, SaturatedConversion},
	transaction_validity::{TransactionLongevity, ValidTransaction},
};

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Config: frame_system::Config {
	/// Because this pallet emits events, it depends on the runtime's definition of an event.
	type Event: From<Event> + Into<<Self as frame_system::Config>::Event>;
	type FindAuthor: FindAuthor<AuraId>;
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
			let reward = Self::validate_transaction(&tx)?;

			Self::update_storage(&tx, reward)?;

			// 3. emit success event
			Self::deposit_event(Event::TransactionSuccess(tx));

			Ok(())
		}

		// function executed at the end of each block
		fn on_finalize() {
			let digest = <frame_system::Module<T>>::digest();
			let pre_runtime_digests = digest.logs.iter().filter_map(|d| d.as_pre_runtime());

			match T::FindAuthor::find_author(pre_runtime_digests) {
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
	pub fn get_simple_tx(tx: &Transaction) -> Vec<u8> {
		let mut tx = tx.clone();

		for input in tx.inputs.iter_mut() {
			input.sigscript = H512::zero();
		}

		tx.encode()
	}

	/// 1. Inputs and Outputs are not empty
	/// 2. Each Input exists and is used exactly once
	/// 3. Each Output is defined exactly once and has nonzero value
	/// 4. Total Output value must not exceed total Input value
	/// 5. New Outputs do not collide with existing ones
	/// 6. Replay attacks are not possible
	/// 7. Provided Input signatures are valid
	/// 	- The Input UTXO is indeed signed by the owner
	///   - Transactions are tamperproof
	pub fn validate_transaction(tx: &Transaction) -> Result<Value, &'static str> {
		ensure!(!tx.inputs.is_empty(), "no inputs");
		ensure!(!tx.outputs.is_empty(), "no outputs");

		// use btree map to dedupe same inputs
		let input_set: BTreeMap<_, ()> = tx.inputs.iter().map(|input| (input, ())).collect();
		ensure!(input_set.len() == tx.inputs.len(), "Each input must be used once");

		let output_set: BTreeMap<_, ()> = tx.outputs.iter().map(|output| (output, ())).collect();
		ensure!(output_set.len() == tx.outputs.len(), "Each output must be defined only once");

		let simple_transaction = Self::get_simple_tx(&tx);
		let mut total_input: Value = 0;
		let mut total_output: Value = 0;

		for input in tx.inputs.iter() {
			if let Some(input_utxo) = UtxoStore::get(&input.outpoint) {
				// check sigs
				ensure!(
					sp_io::crypto::sr25519_verify(
						&Signature::from_raw(*input.sigscript.as_fixed_bytes()),
						&simple_transaction,
						&Public::from_h256(input_utxo.pubkey)
					),
					"Signature must be valid"
				);

				total_input = total_input.checked_add(input_utxo.value).ok_or("input value overflow")?;
			} else {
				// TODO
			}
		}

		let mut output_index: u64 = 0;
		for output in tx.outputs.iter() {
			ensure!(output.value > 0, "output valud must be nonzero");
			let hash = BlakeTwo256::hash_of(&(&tx.encode(), output_index));
			output_index = output_index.checked_add(1).ok_or("output index overflow")?;
			ensure!(!UtxoStore::contains_key(hash), "output already exists");

			total_output = total_output.checked_add(output.value).ok_or("output value overflow")?;
		}

		ensure!(total_input >= total_output, "output value must not exceed the input value");
		let reward = total_input.checked_sub(total_output).ok_or("output index overflow")?;

		Ok(reward)
	}

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

	fn disperse_reward(author: &AuraId) {
		let reward = RewardTotal::take();
		let utxo = TransactionOutput{
			value: reward,
			pubkey: H256::from_slice(author.as_slice()),
		};

		let current_block = <frame_system::Module<T>>::block_number().saturated_into::<u64>();
		let hash = BlakeTwo256::hash_of(&(&utxo, current_block));

		// Store the Utxo
		UtxoStore::insert(hash, utxo);

		Self::deposit_event(Event::RewardsIssued(reward, hash));
	}
}
