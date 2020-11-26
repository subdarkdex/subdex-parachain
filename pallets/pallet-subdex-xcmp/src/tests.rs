mod handle_downward_message;

pub use super::*;
pub use crate::mock::*;

// Subdex

pub fn asset_balances(account_id: AccountId, asset_id: AssetId) -> Balance {
    SubDex::asset_balances(account_id, asset_id)
}

// Subdex Xcmp

pub fn asset_id_exists(para_id: ParaId, asset_id: Option<AssetId>) -> bool {
    AssetIdByParaAssetId::<Test>::contains_key(para_id, asset_id)
}

pub fn asset_id_by_para_asset_id(para_id: ParaId, asset_id: Option<AssetId>) -> AssetId {
    SubdexXcmp::asset_id_by_para_asset_id(para_id, asset_id)
}

pub fn next_asset_id() -> AssetId {
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
