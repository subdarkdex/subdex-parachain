
mod handle_downward_message;

pub use super::*;
pub use crate::mock::*;

pub fn emulate_downward_message(dest: AccountId, transfer_amount: Balance) {
    let downward_message = DownwardMessage::TransferInto(dest, transfer_amount, [0u8; 32]);
    SubdexXcmp::handle_downward_message(&downward_message);
}
