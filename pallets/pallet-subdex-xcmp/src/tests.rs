mod handle_downward_message;
mod handle_xcmp_message;
mod initialize_exchange;
mod transfer_balance_to_parachain_chain;
mod transfer_balance_to_relay_chain;

pub use super::*;
pub use crate::mock::*;
use pallet_subdex::Exchange;

// Subdex

pub fn asset_balances(account_id: AccountId, asset_id: AssetId) -> Balance {
    SubDex::asset_balances(account_id, asset_id)
}

pub fn dex_exchanges(first_asset: Asset<AssetId>, second_asset: Asset<AssetId>) -> Exchange<Test> {
    SubDex::exchanges(first_asset, second_asset)
}

pub fn initialize_new_exchange(
    origin: AccountId,
    first_asset: Asset<AssetId>,
    first_asset_amount: Balance,
    second_asset: Asset<AssetId>,
    second_asset_amount: Balance,
) -> DispatchResult {
    SubDex::initialize_exchange(
        Origin::signed(origin),
        first_asset,
        first_asset_amount,
        second_asset,
        second_asset_amount,
    )
}

// Subdex Xcmp

pub fn asset_id_exists(para_id: ParaId, asset_id: Option<AssetId>) -> bool {
    AssetIdByParaAssetId::<Test>::contains_key(para_id, asset_id)
}

pub fn asset_id_by_para_asset_id(para_id: ParaId, asset_id: Option<AssetId>) -> AssetId {
    SubdexXcmp::asset_id_by_para_asset_id(para_id, asset_id)
}

pub fn get_next_asset_id() -> AssetId {
    SubdexXcmp::next_asset_id()
}

pub fn emulate_downward_message(dest: AccountId, transfer_amount: Balance) {
    let downward_message = DownwardMessage::TransferInto(dest, transfer_amount, [0u8; 32]);
    SubdexXcmp::handle_downward_message(&downward_message);
}

pub fn emulate_xcmp_message(
    para_id: ParaId,
    dest: AccountId,
    transfer_amount: Balance,
    asset_id: Option<AssetId>,
) {
    let xcmp_message = XCMPMessage::TransferToken(dest, transfer_amount, asset_id);
    SubdexXcmp::handle_xcmp_message(para_id, &xcmp_message);
}

pub fn emulate_transfer_balance_to_relay_chain(
    origin: AccountId,
    dest: AccountId,
    transfer_amount: Balance,
) -> DispatchResult {
    SubdexXcmp::transfer_balance_to_relay_chain(Origin::signed(origin), dest, transfer_amount)
}

pub fn emulate_transfer_asset_balance_to_parachain_chain(
    origin: AccountId,
    para_id: ParaId,
    dest: AccountId,
    para_asset_id: Option<AssetId>,
    transfer_amount: Balance,
) -> DispatchResult {
    SubdexXcmp::transfer_asset_balance_to_parachain_chain(
        Origin::signed(origin),
        para_id.into(),
        dest,
        para_asset_id,
        transfer_amount,
    )
}
