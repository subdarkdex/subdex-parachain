#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Codec, Decode, Encode};
use frame_support::traits::{Currency, Imbalance};
use frame_support::{
    decl_error, decl_event, decl_module, decl_storage, dispatch, ensure,
    traits::{Get, WithdrawReason},
    Parameter,
};
use frame_system::{self as system, ensure_signed};
use sp_arithmetic::traits::{BaseArithmetic, Zero};
use sp_runtime::traits::{
    CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, MaybeSerializeDeserialize, Member,
};

use sp_std::{collections::btree_map::BTreeMap, fmt::Debug, prelude::*};

mod exchange;
use exchange::Exchange;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use serde::{Deserialize, Serialize};

pub type BalanceOf<T> =
    <<T as Trait>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;

pub trait Trait: system::Trait + balances::Trait {
    type Event: From<Event<Self>>
        + Into<<Self as system::Trait>::Event>
        + Into<<Self as balances::Trait>::Event>;

    type Currency: Currency<Self::AccountId>;

    type InitialShares: Get<BalanceOf<Self>>;

    // Id representation for assets, located on other parachains.
    // Some ids can be reserved to specify internal assets.
    type AssetId: Parameter
        + Member
        + BaseArithmetic
        + Codec
        + Default
        + Copy
        + Clone
        + MaybeSerializeDeserialize
        + Eq
        + PartialEq
        + Ord;

    type KSMAssetId: Get<Self::AssetId>;

    type FeeRate: Get<BalanceOf<Self>>;

    type ExchangeFeeRateNominator: Get<BalanceOf<Self>>;

    type ExchangeFeeRateDenominator: Get<BalanceOf<Self>>;
}

decl_storage! {
    trait Store for Module<T: Trait> as TemplateModule {
        pub Exchanges get(fn exchanges): double_map hasher(blake2_128_concat) T::AssetId, hasher(blake2_128_concat) T::AssetId => Exchange<T>;

        // Balances of assets, located on other parachains.
        pub AssetBalances get(fn asset_balances): double_map hasher(blake2_128_concat) T::AccountId, hasher(blake2_128_concat) T::AssetId => BalanceOf<T>;

        // Treasury account
        pub DEXAccountId get(fn dex_account_id) config(): T::AccountId;

        // Next asset id
        pub NextAssetId get(fn next_asset_id) config(): T::AssetId;
    }
}

decl_event!(
    pub enum Event<T>
    where
        AccountId = <T as system::Trait>::AccountId,
        AssetId = <T as Trait>::AssetId,
        Shares = BalanceOf<T>,
        Balance = BalanceOf<T>,
    {
        Exchanged(AssetId, Balance, AssetId, Balance, AccountId),
        Invested(AccountId, AssetId, AssetId, Shares),
        Divested(AccountId, AssetId, AssetId, Shares),
        Withdrawn(AssetId, Balance, AccountId),
    }
);

decl_error! {
    pub enum Error for Module<T: Trait> {
        ExchangeNotExists,
        ExchangeAlreadyExists,
        InvalidExchange,
        InvariantNotNull,
        TotalSharesNotNull,
        LowKsmAmount,
        LowTokenAmount,
        KsmAmountBelowExpectation,
        TokenAmountBelowExpectation,
        InsufficientPool,
        InvalidShares,
        InsufficientShares,
        DoesNotOwnShare,
        InsufficientKsmBalance,
        InsufficientOtherAssetBalance,

        OverflowOccured
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        type Error = Error<T>;

        fn deposit_event() = default;

        #[weight = 10_000]
        pub fn initialize_exchange(origin, first_asset_id: T::AssetId, first_asset_amount: BalanceOf<T>, second_asset_id: T::AssetId, second_asset_amount: BalanceOf<T>) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, first_asset_amount, second_asset_id, second_asset_amount) =
                Self::adjust_assets_amount_order(first_asset_id, first_asset_amount, second_asset_id, second_asset_amount);

            ensure!(
                first_asset_amount > BalanceOf::<T>::zero(),
                Error::<T>::LowKsmAmount
            );
            ensure!(
                second_asset_amount > BalanceOf::<T>::zero(),
                Error::<T>::LowTokenAmount
            );

            Self::ensure_exchange_not_exists(first_asset_id, second_asset_id)?;
            Self::exchanges(first_asset_id, second_asset_id).ensure_launch()?;
            Self::ensure_sufficient_balances(&sender, first_asset_id, first_asset_amount, second_asset_id, second_asset_amount)?;

            //
            // == MUTATION SAFE ==
            //

            Self::slash_assets(&sender, first_asset_id, first_asset_amount, second_asset_id, second_asset_amount);

            // TODO adjust shares allocation
            let exchange = Exchange::<T>::initialize_new(first_asset_amount, second_asset_amount, sender.clone());

            Exchanges::<T>::insert(first_asset_id, second_asset_id, exchange);

            Self::deposit_event(RawEvent::Invested(sender, first_asset_id, second_asset_id, T::InitialShares::get()));
            Ok(())
        }

        // #[weight = 10_000]
        // pub fn swap(
        //     origin,
        //     token_in: T::AssetId,
        //     token_in_amount: BalanceOf<T>,
        //     token_out: T::AssetId,
        //     min_token_out_amount: BalanceOf<T>,
        //     receiver: T::AccountId
        // ) -> dispatch::DispatchResult {
        //     let sender = ensure_signed(origin)?;

        //     Self::ensure_valid_exchange(token_in, token_out)?;

        //     let from_exchange = Self::ensure_exchange_exists(token_in)?;

        //     let to_exchange = Self::ensure_exchange_exists(token_out)?;

        //     let (new_ksm_pool_from, new_token_pool_from, first_currency_amount) =
        //         from_exchange.calculate_token_to_ksm_swap(token_in_amount);
        //     from_exchange.ensure_ksm_amount(first_currency_amount, BalanceOf::<T>::zero())?;

        //     let (new_ksm_pool_to, new_token_pool_to, token_out_amount) =
        //         to_exchange.calculate_ksm_to_token_swap(first_currency_amount);
        //     to_exchange.ensure_token_amount(token_out_amount, min_token_out_amount)?;
        //     Self::ensure_sufficient_balance(&sender, &token_in, token_in_amount)?;
        //     Self::ensure_sufficient_balance(&Self::dex_account_id(), &token_out, token_out_amount)?;

        //     //
        //     // == MUTATION SAFE ==
        //     //

        //     // transfer `second_currency_amount` to the DEX account
        //     <balances::Module<T>>::make_transfer_with_event(
        //         &token_in,
        //         &sender,
        //         &Self::dex_account_id(),
        //         token_in_amount,
        //     )?;
        //     // transfer `tokens_out` to the receiver
        //     <balances::Module<T>>::make_transfer_with_event(
        //         &token_out,
        //         &Self::dex_account_id(),
        //         &receiver,
        //         token_out_amount,
        //     )?;

        //     <Exchanges<T>>::mutate(token_in, |exchange| {
        //         exchange.update_pools(new_ksm_pool_from, new_token_pool_from)
        //     });
        //     <Exchanges<T>>::mutate(token_out, |exchange| {
        //         exchange.update_pools(new_ksm_pool_to, new_token_pool_to)
        //     });

        //     Self::deposit_event(RawEvent::Exchanged(
        //         token_in,
        //         token_in_amount,
        //         token_out,
        //         token_out_amount,
        //         sender,
        //     ));
        //     Ok(())
        // }

        #[weight = 10_000]
        pub fn invest_liquidity(origin, first_asset_id: T::AssetId, second_asset_id: T::AssetId, shares: BalanceOf<T>) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, second_asset_id) =
                Self::adjust_assets_order(first_asset_id, second_asset_id);

            let (first_asset_cost, second_asset_cost) = Self::ensure_exchange_exists(first_asset_id, second_asset_id)?.calculate_costs(shares);
            Self::ensure_sufficient_balances(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost)?;

            //
            // == MUTATION SAFE ==
            //

            Self::slash_assets(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost);


            <Exchanges<T>>::mutate(first_asset_id, second_asset_id, |exchange| {
                exchange.invest(first_asset_cost, second_asset_cost, shares, &sender)
            });

            Self::deposit_event(RawEvent::Invested(sender, first_asset_id, second_asset_id, shares));
            Ok(())
        }

        #[weight = 10_000]
        pub fn divest_liquidity(
            origin,
            first_asset_id: T::AssetId,
            second_asset_id: T::AssetId,
            shares_burned:  BalanceOf<T>,
            min_first_asset_received: BalanceOf<T>,
            min_second_asset_received: BalanceOf<T>
        ) -> dispatch::DispatchResult {
            let sender = ensure_signed(origin)?;

            let (first_asset_id, second_asset_id) = Self::adjust_assets_order(first_asset_id, second_asset_id);

            let exchange = Self::ensure_exchange_exists(first_asset_id, second_asset_id)?;
            exchange.ensure_burned_shares(&sender, shares_burned)?;

            let (first_asset_cost, second_asset_cost) = exchange.calculate_costs(shares_burned);
            Self::ensure_divest_expectations(first_asset_cost, second_asset_cost, min_first_asset_received, min_second_asset_received)?;

            //
            // == MUTATION SAFE ==
            //

            Self::mint_assets(&sender, first_asset_id, first_asset_cost, second_asset_id, second_asset_cost)?;

            <Exchanges<T>>::mutate(first_asset_id, second_asset_id, |exchange| {
                exchange.divest(first_asset_cost, second_asset_cost, shares_burned, &sender)
            });

            Self::deposit_event(RawEvent::Divested(sender, first_asset_id, second_asset_id, shares_burned));
            Ok(())
        }
    }
}

impl<T: Trait> Module<T> {
    pub fn ensure_valid_exchange(
        token_in: T::AssetId,
        token_out: T::AssetId,
    ) -> dispatch::DispatchResult {
        ensure!(token_in != token_out, Error::<T>::InvalidExchange);
        Ok(())
    }

    pub fn slash_assets(
        from: &T::AccountId,
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) {
        // TODO
        // Refactor, when we`ll have native support for multiple currencies.
        match (first_asset_id, second_asset_id) {
            (first_asset_id, _) if first_asset_id == T::KSMAssetId::get() => {
                let _ = T::Currency::slash(from, first_asset_amount);
                <AssetBalances<T>>::mutate(from, second_asset_id, |second_asset_balance| {
                    *second_asset_balance -= second_asset_amount
                });
            }
            (_, second_asset_id) if second_asset_id == T::KSMAssetId::get() => {
                let _ = T::Currency::slash(from, second_asset_amount);
                <AssetBalances<T>>::mutate(from, first_asset_id, |first_asset_balance| {
                    *first_asset_balance -= first_asset_amount
                });
            }
            _ => {
                <AssetBalances<T>>::mutate(from, first_asset_id, |first_asset_balance| {
                    *first_asset_balance -= first_asset_amount
                });

                <AssetBalances<T>>::mutate(from, second_asset_id, |second_asset_balance| {
                    *second_asset_balance -= second_asset_amount
                });
            }
        }
    }

    pub fn mint_assets(
        to: &T::AccountId,
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) -> Result<(), Error<T>> {
        // TODO
        // Refactor, when we`ll have native support for multiple currencies.
        // Use try_mutate to avoid potential overflow risks.
        match (first_asset_id, second_asset_id) {
            (first_asset_id, _) if first_asset_id == T::KSMAssetId::get() => {
                ensure!(
                    T::Currency::deposit_creating(to, first_asset_amount).peek()
                        != BalanceOf::<T>::zero(),
                    Error::<T>::OverflowOccured
                );
                <AssetBalances<T>>::try_mutate(to, second_asset_id, |second_asset_balance| {
                    if let Some(total) = second_asset_balance.checked_add(&second_asset_amount) {
                        *second_asset_balance = total;
                        Ok(())
                    } else {
                        Err(Error::<T>::OverflowOccured)
                    }
                })
            }
            (_, second_asset_id) if second_asset_id == T::KSMAssetId::get() => {
                ensure!(
                    T::Currency::deposit_creating(to, first_asset_amount).peek()
                        != BalanceOf::<T>::zero(),
                    Error::<T>::OverflowOccured
                );
                <AssetBalances<T>>::try_mutate(to, first_asset_id, |first_asset_balance| {
                    if let Some(total) = first_asset_balance.checked_add(&first_asset_amount) {
                        *first_asset_balance = total;
                        Ok(())
                    } else {
                        Err(Error::<T>::OverflowOccured)
                    }
                })
            }
            _ => {
                <AssetBalances<T>>::try_mutate(to, first_asset_id, |first_asset_balance| {
                    if let Some(total) = first_asset_balance.checked_add(&first_asset_amount) {
                        *first_asset_balance = total;
                        Ok(())
                    } else {
                        Err(Error::<T>::OverflowOccured)
                    }
                })?;

                <AssetBalances<T>>::try_mutate(to, second_asset_id, |second_asset_balance| {
                    if let Some(total) = second_asset_balance.checked_add(&second_asset_amount) {
                        *second_asset_balance = total;
                        Ok(())
                    } else {
                        Err(Error::<T>::OverflowOccured)
                    }
                })
            }
        }
    }

    pub fn ensure_exchange_exists(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> Result<Exchange<T>, Error<T>> {
        let exchange = Self::exchanges(first_asset_id, second_asset_id);

        ensure!(
            exchange.invariant > BalanceOf::<T>::zero(),
            Error::<T>::ExchangeNotExists
        );
        Ok(exchange)
    }

    pub fn adjust_assets_amount_order(
        first_asset_id: T::AssetId,
        first_asset_amount: BalanceOf<T>,
        second_asset_id: T::AssetId,
        second_asset_amount: BalanceOf<T>,
    ) -> (T::AssetId, BalanceOf<T>, T::AssetId, BalanceOf<T>) {
        if first_asset_id > second_asset_id {
            (
                second_asset_id,
                second_asset_amount,
                first_asset_id,
                first_asset_amount,
            )
        } else {
            (
                first_asset_id,
                first_asset_amount,
                second_asset_id,
                second_asset_amount,
            )
        }
    }

    pub fn adjust_assets_order(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> (T::AssetId, T::AssetId) {
        if first_asset_id > second_asset_id {
            (second_asset_id, first_asset_id)
        } else {
            (first_asset_id, second_asset_id)
        }
    }

    pub fn ensure_exchange_not_exists(
        first_asset_id: T::AssetId,
        second_asset_id: T::AssetId,
    ) -> dispatch::DispatchResult {
        let first_exchange = Self::exchanges(first_asset_id, second_asset_id);

        ensure!(
            first_exchange.invariant == BalanceOf::<T>::zero(),
            Error::<T>::ExchangeAlreadyExists
        );
        Ok(())
    }

    pub fn ensure_sufficient_balances(
        sender: &T::AccountId,
        token_in: T::AssetId,
        token_in_amount: BalanceOf<T>,
        token_out: T::AssetId,
        token_out_amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        Self::ensure_sufficient_balance(sender, token_in, token_in_amount)?;
        Self::ensure_sufficient_balance(sender, token_out, token_out_amount)?;
        Ok(())
    }

    pub fn ensure_sufficient_balance(
        from: &T::AccountId,
        asset_id: T::AssetId,
        amount: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        match asset_id {
            // Here we also can add other currencies, with native dex parachain support.
            asset_id if asset_id == T::KSMAssetId::get() => {
                let new_balance = T::Currency::free_balance(from)
                    .checked_sub(&amount)
                    .ok_or(Error::<T>::InsufficientKsmBalance)?;

                T::Currency::ensure_can_withdraw(
                    from,
                    amount,
                    WithdrawReason::Transfer.into(),
                    new_balance,
                )?;
                Ok(())
            }
            asset_id if Self::asset_balances(from, asset_id) >= amount => Ok(()),
            _ => Err(Error::<T>::InsufficientOtherAssetBalance.into()),
        }
    }

    pub fn ensure_divest_expectations(
        first_asset_cost: BalanceOf<T>,
        second_asset_cost: BalanceOf<T>,
        min_first_asset_received: BalanceOf<T>,
        min_second_asset_received: BalanceOf<T>,
    ) -> dispatch::DispatchResult {
        ensure!(
            first_asset_cost >= min_first_asset_received,
            Error::<T>::KsmAmountBelowExpectation
        );
        ensure!(
            second_asset_cost >= min_second_asset_received,
            Error::<T>::TokenAmountBelowExpectation
        );
        Ok(())
    }
}
