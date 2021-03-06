// Copyright 2020 Parity Technologies (UK) Ltd.

use cumulus_primitives::ParaId;
use parachain_runtime::{
    AccountId, DexXCMPConfig, GenesisConfig, Signature, SudoConfig, SystemConfig, WASM_BINARY,
};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<GenesisConfig, Extensions>;

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{}", seed), None)
        .expect("static values are valid; qed")
        .public()
}

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
    /// The relay chain of the Parachain.
    pub relay_chain: String,
    /// The id of the Parachain.
    pub para_id: u32,
}

impl Extensions {
    /// Try to get the extension from the given `ChainSpec`.
    pub fn try_get(chain_spec: &Box<dyn sc_service::ChainSpec>) -> Option<&Self> {
        sc_chain_spec::get_extension(chain_spec.extensions())
    }
}

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn get_chain_spec(id: ParaId) -> ChainSpec {
    ChainSpec::from_genesis(
        "Subdex Parachain Network",
        "local_testnet",
        ChainType::Local,
        move || testnet_genesis(get_account_id_from_seed::<sr25519::Public>("Alice"), id),
        vec![],
        None,
        None,
        None,
        Extensions {
            relay_chain: "local_testnet".into(),
            para_id: id.into(),
        },
    )
}

pub fn staging_test_net(id: ParaId) -> ChainSpec {
    ChainSpec::from_genesis(
        "Subdex Staging Testnet",
        "staging_testnet",
        ChainType::Live,
        move || testnet_genesis(get_account_id_from_seed::<sr25519::Public>("Alice"), id),
        Vec::new(),
        None,
        None,
        None,
        Extensions {
            relay_chain: "rococo_local_testnet".into(),
            para_id: id.into(),
        },
    )
}

fn testnet_genesis(root_key: AccountId, _id: ParaId) -> GenesisConfig {
    GenesisConfig {
        frame_system: Some(SystemConfig {
            code: WASM_BINARY.to_vec(),
            changes_trie_config: Default::default(),
        }),
        pallet_sudo: Some(SudoConfig {
            key: root_key.clone(),
        }),
        pallet_subdex_xcmp: Some(DexXCMPConfig { next_asset_id: 1 }),
    }
}
